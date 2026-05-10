const std = @import("std");

const git = @import("git.zig");
const log = @import("log.zig");
const actions = @import("../syntax/github_actions.zig");

const Version = git.Version;

pub const UpdateMode = enum {
    major,
    minor,
    patch,
};

pub const PinStyle = enum {
    sha,
    preserve,
};

pub const ResolveOptions = struct {
    excludes: []const []const u8,
    include_branches: bool,
    mode: UpdateMode,
    style: PinStyle,
};

pub const Candidate = struct {
    action: []const u8,
    job: []const u8,
    current: []const u8,
    current_ref: []const u8 = "",
    version_comment: []const u8 = "",
    sha_mismatch: bool = false,
    next: []const u8,
    next_label: []const u8 = "",
    next_is_major: bool = false,
    file: []const u8,
    line: u32,
    ref_start: u32,
    ref_end: u32,

    pub fn displayTarget(self: Candidate) []const u8 {
        return if (self.next_label.len > 0) self.next_label else self.next;
    }

    pub fn hasCurrentRef(self: Candidate) bool {
        return self.current_ref.len > 0;
    }

    pub fn hasVersionComment(self: Candidate) bool {
        return self.version_comment.len > 0;
    }

    pub fn hasShaMismatch(self: Candidate) bool {
        return self.sha_mismatch;
    }

    pub fn isMajorUpdate(self: Candidate) bool {
        return self.next_is_major;
    }

    pub fn shouldWriteVersionComment(self: Candidate) bool {
        const target = self.displayTarget();
        return target.len > 0 and
            (!std.mem.eql(u8, self.next, target) or self.hasVersionComment() or self.hasShaMismatch());
    }
};

pub fn deinitCandidates(allocator: std.mem.Allocator, candidates: []const Candidate) void {
    for (candidates) |candidate| {
        allocator.free(candidate.action);
        allocator.free(candidate.job);
        allocator.free(candidate.current);
        if (candidate.current_ref.len > 0) allocator.free(candidate.current_ref);
        if (candidate.version_comment.len > 0) allocator.free(candidate.version_comment);
        allocator.free(candidate.next);
        if (candidate.next_label.len > 0) allocator.free(candidate.next_label);
        allocator.free(candidate.file);
    }
    allocator.free(candidates);
}

pub const ResolveError = error{
    GitHubRequestFailed,
    NoTagsFound,
} || std.mem.Allocator.Error || std.http.Client.FetchError || std.json.ParseError(std.json.Scanner);

pub const Diagnostics = struct {
    repository: []const u8 = "",
    status: ?std.http.Status = null,
    cause: []const u8 = "",

    pub fn reset(self: *Diagnostics) void {
        self.* = .{};
    }
};

const RepositoryContext = struct {
    pub fn hash(_: RepositoryContext, repository: actions.Repository) u64 {
        var hasher = std.hash.Wyhash.init(0);
        hasher.update(repository.owner);
        hasher.update(&[_]u8{0});
        hasher.update(repository.name);
        return hasher.final();
    }

    pub fn eql(_: RepositoryContext, lhs: actions.Repository, rhs: actions.Repository) bool {
        return std.mem.eql(u8, lhs.owner, rhs.owner) and std.mem.eql(u8, lhs.name, rhs.name);
    }
};

const ApiTag = struct {
    name: []const u8,
    commit: struct {
        sha: []const u8,
    },
};

const Tag = struct {
    name: []const u8,
    sha: []const u8,
    version: Version,
};

fn chooseTarget(tags: []const Tag, current: ?Version, mode: UpdateMode) ?Tag {
    var best: ?Tag = null;
    for (tags) |tag| {
        if (current) |current_version| {
            switch (mode) {
                .patch => if (tag.version.major != current_version.major or tag.version.minor != current_version.minor) continue,
                .minor => if (tag.version.major != current_version.major) continue,
                .major => {},
            }
        }

        if (best == null or tag.version.order(best.?.version) == .gt) {
            best = tag;
        }
    }
    return best;
}

fn findCurrentSha(tags: []const Tag, ref: []const u8) ?[]const u8 {
    for (tags) |tag| {
        if (std.mem.eql(u8, tag.name, ref)) return tag.sha;
        if (std.mem.eql(u8, tag.sha, ref)) return tag.sha;
        if (std.mem.startsWith(u8, tag.sha, ref)) return tag.sha;
    }
    return null;
}

fn findTag(tags: []const Tag, name: []const u8) ?Tag {
    for (tags) |tag| {
        if (std.mem.eql(u8, tag.name, name)) return tag;
    }
    return null;
}

fn isExcluded(action: []const u8, excludes: []const []const u8) bool {
    for (excludes) |exclude| {
        if (std.mem.indexOf(u8, action, exclude) != null) return true;
    }
    return false;
}

fn isMajorUpdate(current: ?Version, target: Version) bool {
    if (current) |current_version| {
        return target.major > current_version.major;
    }
    return false;
}

test "choose target respects mode" {
    const tags = [_]Tag{
        .{ .name = "v1.2.3", .sha = "a", .version = .{ .major = 1, .minor = 2, .patch = 3 } },
        .{ .name = "v1.3.0", .sha = "b", .version = .{ .major = 1, .minor = 3, .patch = 0 } },
        .{ .name = "v2.0.0", .sha = "c", .version = .{ .major = 2, .minor = 0, .patch = 0 } },
    };

    try std.testing.expectEqualStrings("v1.2.3", chooseTarget(&tags, .{ .major = 1, .minor = 2, .patch = 0 }, .patch).?.name);
    try std.testing.expectEqualStrings("v1.3.0", chooseTarget(&tags, .{ .major = 1, .minor = 2, .patch = 0 }, .minor).?.name);
    try std.testing.expectEqualStrings("v2.0.0", chooseTarget(&tags, .{ .major = 1, .minor = 2, .patch = 0 }, .major).?.name);
}

pub const Client = struct {
    allocator: std.mem.Allocator,
    client: std.http.Client,
    token: ?[]const u8,

    const Self = @This();
    const BASE_URL = "https://api.github.com";
    const API_VERSION = "2022-11-28";

    pub fn init(allocator: std.mem.Allocator, io: std.Io) Self {
        return .{
            .allocator = allocator,
            .client = std.http.Client{ .allocator = allocator, .io = io },
            .token = tokenFromEnv(),
        };
    }

    pub fn deinit(self: *Self) void {
        self.client.deinit();
    }

    pub fn resolve(
        self: *Self,
        found: []const actions.Reference,
        options: ResolveOptions,
        diagnostics: ?*Diagnostics,
    ) ResolveError![]Candidate {
        if (diagnostics) |diag| diag.reset();

        var candidates: std.ArrayList(Candidate) = .empty;
        errdefer candidates.deinit(self.allocator);

        var cache = std.HashMap(actions.Repository, []Tag, RepositoryContext, std.hash_map.default_max_load_percentage).init(self.allocator);
        defer cache.deinit();

        for (found) |found_reference| {
            const action_display = try found_reference.name.allocDisplay(self.allocator);
            var keep_action_display = false;
            defer if (!keep_action_display) self.allocator.free(action_display);

            if (isExcluded(action_display, options.excludes)) {
                log.debug("resolve skip excluded action={s} file={s}:{d}", .{ action_display, found_reference.source.file, found_reference.source.line });
                continue;
            }

            const comment_version = if (found_reference.version_hint.len > 0) git.parseVersion(found_reference.version_hint) else null;
            const current_version = git.parseVersion(found_reference.current_ref) orelse comment_version;
            const current_is_sha = git.isLikelySha(found_reference.current_ref);
            if (current_version == null and !current_is_sha and !options.include_branches) {
                log.debug("resolve skip unversioned action={s} ref={s} file={s}:{d}", .{ action_display, found_reference.current_ref, found_reference.source.file, found_reference.source.line });
                continue;
            }

            const repository = found_reference.name.repository;
            var cache_hit = true;
            const tags = if (cache.get(repository)) |cached| cached else blk: {
                cache_hit = false;
                log.debug("github fetch tags repo={s}/{s}", .{ repository.owner, repository.name });
                const fetched = try self.fetchTags(repository, diagnostics);
                try cache.put(repository, fetched);
                log.debug("github fetched tags repo={s}/{s} version_tags={d}", .{ repository.owner, repository.name, fetched.len });
                break :blk fetched;
            };
            if (cache_hit) {
                log.debug("github cache hit repo={s}/{s} version_tags={d}", .{ repository.owner, repository.name, tags.len });
            }

            const commented_tag = if (found_reference.version_hint.len > 0) findTag(tags, found_reference.version_hint) else null;
            const sha_mismatch = current_is_sha and commented_tag != null and !git.shaMatches(found_reference.current_ref, commented_tag.?.sha);
            const target = chooseTarget(tags, current_version, options.mode) orelse {
                log.debug("resolve skip no target action={s} ref={s} mode={s} file={s}:{d}", .{
                    action_display,
                    found_reference.current_ref,
                    @tagName(options.mode),
                    found_reference.source.file,
                    found_reference.source.line,
                });
                continue;
            };
            const current_ref = if (commented_tag) |tag|
                tag.sha
            else
                findCurrentSha(tags, found_reference.current_ref) orelse if (current_is_sha) found_reference.current_ref else "";

            if (!sha_mismatch and (std.mem.eql(u8, found_reference.current_ref, target.name) or std.mem.eql(u8, found_reference.current_ref, target.sha))) {
                log.debug("resolve skip current action={s} ref={s} target={s} file={s}:{d}", .{
                    action_display,
                    found_reference.current_ref,
                    target.name,
                    found_reference.source.file,
                    found_reference.source.line,
                });
                continue;
            }

            log.debug("resolve candidate action={s} current={s} target={s} sha_mismatch={} major={} file={s}:{d}", .{
                action_display,
                found_reference.current_ref,
                target.name,
                sha_mismatch,
                isMajorUpdate(current_version, target.version),
                found_reference.source.file,
                found_reference.source.line,
            });
            try candidates.append(self.allocator, .{
                .action = action_display,
                .job = try self.allocator.dupe(u8, found_reference.scope),
                .current = try self.allocator.dupe(u8, found_reference.current_ref),
                .current_ref = if (current_ref.len > 0) try self.allocator.dupe(u8, current_ref) else "",
                .version_comment = if (found_reference.version_hint.len > 0) try self.allocator.dupe(u8, found_reference.version_hint) else "",
                .sha_mismatch = sha_mismatch,
                .next = try self.allocator.dupe(u8, if (options.style == .sha) target.sha else target.name),
                .next_label = try self.allocator.dupe(u8, target.name),
                .next_is_major = isMajorUpdate(current_version, target.version),
                .file = try self.allocator.dupe(u8, found_reference.source.file),
                .line = found_reference.source.line,
                .ref_start = found_reference.source.ref_span.start,
                .ref_end = found_reference.source.ref_span.end,
            });
            keep_action_display = true;
        }

        return candidates.toOwnedSlice(self.allocator);
    }

    fn fetchTags(
        self: *Self,
        repository: actions.Repository,
        diagnostics: ?*Diagnostics,
    ) ResolveError![]Tag {
        const path = try std.fmt.allocPrint(self.allocator, "/repos/{s}/{s}/tags?per_page=100", .{ repository.owner, repository.name });
        defer self.allocator.free(path);

        var body = std.Io.Writer.Allocating.init(self.allocator);
        defer body.deinit();

        const status = self.get(path, &body.writer) catch |err| {
            if (diagnostics) |diag| {
                diag.repository = try repository.allocDisplay(self.allocator);
                diag.cause = @errorName(err);
            }
            return err;
        };

        if (status.class() != .success) {
            if (diagnostics) |diag| {
                diag.repository = try repository.allocDisplay(self.allocator);
                diag.status = status;
            }
            return error.GitHubRequestFailed;
        }

        return self.parseTags(repository, body.written(), diagnostics);
    }

    fn parseTags(self: *Self, repository: actions.Repository, body: []const u8, diagnostics: ?*Diagnostics) ResolveError![]Tag {
        const parsed = try std.json.parseFromSlice([]ApiTag, self.allocator, body, .{
            .ignore_unknown_fields = true,
            .allocate = .alloc_always,
        });
        defer parsed.deinit();

        var tags: std.ArrayList(Tag) = .empty;
        errdefer tags.deinit(self.allocator);

        for (parsed.value) |api_tag| {
            const parsed_version = git.parseVersion(api_tag.name) orelse continue;
            try tags.append(self.allocator, .{
                .name = try self.allocator.dupe(u8, api_tag.name),
                .sha = try self.allocator.dupe(u8, api_tag.commit.sha),
                .version = parsed_version,
            });
        }

        if (tags.items.len == 0) {
            if (diagnostics) |diag| {
                diag.repository = try repository.allocDisplay(self.allocator);
            }
            return error.NoTagsFound;
        }
        return tags.toOwnedSlice(self.allocator);
    }

    fn get(self: *Self, path: []const u8, response_writer: *std.Io.Writer) !std.http.Status {
        const url = try std.fmt.allocPrint(self.allocator, "{s}{s}", .{ BASE_URL, path });
        defer self.allocator.free(url);

        const headers = [_]std.http.Header{
            .{ .name = "Accept", .value = "application/vnd.github+json" },
            .{ .name = "User-Agent", .value = "actioneer" },
            .{ .name = "X-GitHub-Api-Version", .value = API_VERSION },
        };
        const auth_header_value = if (self.token) |token| try std.fmt.allocPrint(self.allocator, "Bearer {s}", .{token}) else "";
        defer if (self.token != null) self.allocator.free(auth_header_value);
        const auth_headers = [_]std.http.Header{
            .{ .name = "Accept", .value = "application/vnd.github+json" },
            .{ .name = "User-Agent", .value = "actioneer" },
            .{ .name = "X-GitHub-Api-Version", .value = API_VERSION },
            .{ .name = "Authorization", .value = auth_header_value },
        };

        const result = try self.client.fetch(.{
            .location = .{ .url = url },
            .response_writer = response_writer,
            .extra_headers = if (self.token == null) &headers else &auth_headers,
        });
        return result.status;
    }
};

fn tokenFromEnv() ?[]const u8 {
    const value = std.c.getenv("GITHUB_TOKEN") orelse return null;
    const slice = std.mem.sliceTo(value, 0);
    if (slice.len == 0) return null;
    return slice;
}
