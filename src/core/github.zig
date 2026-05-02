const std = @import("std");

const config = @import("config.zig");
const types = @import("types.zig");
const updates = @import("updates.zig");

pub const ResolveError = error{
    GitHubRequestFailed,
    NoTagsFound,
} || std.mem.Allocator.Error || std.http.Client.FetchError || std.json.ParseError(std.json.Scanner);

pub const Diagnostics = struct {
    repo: []const u8 = "",
    status: ?std.http.Status = null,
    cause: []const u8 = "",

    pub fn reset(self: *Diagnostics) void {
        self.* = .{};
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

const Version = struct {
    major: u32,
    minor: u32,
    patch: u32,

    fn order(lhs: Version, rhs: Version) std.math.Order {
        if (lhs.major != rhs.major) return std.math.order(lhs.major, rhs.major);
        if (lhs.minor != rhs.minor) return std.math.order(lhs.minor, rhs.minor);
        return std.math.order(lhs.patch, rhs.patch);
    }
};

pub fn resolve(
    allocator: std.mem.Allocator,
    io: std.Io,
    found: []const types.FoundAction,
    parsed: config.Config,
    diagnostics: ?*Diagnostics,
) ResolveError![]updates.Candidate {
    if (diagnostics) |diag| diag.reset();

    var client = std.http.Client{
        .allocator = allocator,
        .io = io,
    };
    defer client.deinit();

    var candidates: std.ArrayList(updates.Candidate) = .empty;
    errdefer candidates.deinit(allocator);

    var cache = std.StringHashMap([]Tag).init(allocator);
    defer cache.deinit();

    for (found) |action| {
        if (isExcluded(action.action, parsed.excludes)) continue;

        const comment_version = if (action.version_comment.len > 0) parseVersion(action.version_comment) else null;
        const current_version = parseVersion(action.ref) orelse comment_version;
        const current_is_sha = isLikelySha(action.ref);
        if (current_version == null and !current_is_sha and !parsed.include_branches) continue;

        const repo_key = try std.fmt.allocPrint(allocator, "{s}/{s}", .{ action.owner, action.repo });
        const tags = if (cache.get(repo_key)) |cached| cached else blk: {
            const fetched = try fetchTags(allocator, &client, action.owner, action.repo, repo_key, parsed.github_token, diagnostics);
            try cache.put(repo_key, fetched);
            break :blk fetched;
        };

        const commented_tag = if (action.version_comment.len > 0) findTag(tags, action.version_comment) else null;
        const sha_mismatch = current_is_sha and commented_tag != null and !shaMatches(action.ref, commented_tag.?.sha);
        const target = chooseTarget(tags, current_version, parsed.mode) orelse continue;
        const current_ref = if (commented_tag) |tag|
            tag.sha
        else
            findCurrentSha(tags, action.ref) orelse if (current_is_sha) action.ref else "";

        if (!sha_mismatch and (std.mem.eql(u8, action.ref, target.name) or std.mem.eql(u8, action.ref, target.sha))) {
            continue;
        }

        try candidates.append(allocator, .{
            .action = action.action,
            .job = action.job,
            .current = action.ref,
            .current_ref = current_ref,
            .version_comment = action.version_comment,
            .sha_mismatch = sha_mismatch,
            .next = if (std.mem.eql(u8, parsed.style, "sha")) target.sha else target.name,
            .next_label = target.name,
            .next_is_major = isMajorUpdate(current_version, target.version),
            .file = action.file,
            .line = action.line,
        });
    }

    return candidates.toOwnedSlice(allocator);
}

fn fetchTags(
    allocator: std.mem.Allocator,
    client: *std.http.Client,
    owner: []const u8,
    repo: []const u8,
    repo_key: []const u8,
    github_token: ?[]const u8,
    diagnostics: ?*Diagnostics,
) ResolveError![]Tag {
    const url = try std.fmt.allocPrint(allocator, "https://api.github.com/repos/{s}/{s}/tags?per_page=100", .{ owner, repo });

    var body = std.Io.Writer.Allocating.init(allocator);
    defer body.deinit();

    const headers = [_]std.http.Header{
        .{ .name = "Accept", .value = "application/vnd.github+json" },
        .{ .name = "User-Agent", .value = "actioneer" },
        .{ .name = "X-GitHub-Api-Version", .value = "2022-11-28" },
    };
    const auth_header_value = if (github_token) |token| try std.fmt.allocPrint(allocator, "Bearer {s}", .{token}) else "";
    const auth_headers = [_]std.http.Header{
        .{ .name = "Accept", .value = "application/vnd.github+json" },
        .{ .name = "User-Agent", .value = "actioneer" },
        .{ .name = "X-GitHub-Api-Version", .value = "2022-11-28" },
        .{ .name = "Authorization", .value = auth_header_value },
    };

    const result = client.fetch(.{
        .location = .{ .url = url },
        .response_writer = &body.writer,
        .extra_headers = if (github_token == null) &headers else &auth_headers,
    }) catch |err| {
        if (diagnostics) |diag| {
            diag.repo = repo_key;
            diag.cause = @errorName(err);
        }
        return err;
    };

    if (result.status.class() != .success) {
        if (diagnostics) |diag| {
            diag.repo = repo_key;
            diag.status = result.status;
        }
        return error.GitHubRequestFailed;
    }

    const parsed = try std.json.parseFromSlice([]ApiTag, allocator, body.written(), .{
        .ignore_unknown_fields = true,
        .allocate = .alloc_always,
    });
    defer parsed.deinit();

    var tags: std.ArrayList(Tag) = .empty;
    errdefer tags.deinit(allocator);

    for (parsed.value) |api_tag| {
        const version = parseVersion(api_tag.name) orelse continue;
        try tags.append(allocator, .{
            .name = try allocator.dupe(u8, api_tag.name),
            .sha = try allocator.dupe(u8, api_tag.commit.sha),
            .version = version,
        });
    }

    if (tags.items.len == 0) {
        if (diagnostics) |diag| {
            diag.repo = repo_key;
        }
        return error.NoTagsFound;
    }
    return tags.toOwnedSlice(allocator);
}

fn chooseTarget(tags: []const Tag, current: ?Version, mode: []const u8) ?Tag {
    var best: ?Tag = null;
    for (tags) |tag| {
        if (current) |current_version| {
            if (std.mem.eql(u8, mode, "patch")) {
                if (tag.version.major != current_version.major or tag.version.minor != current_version.minor) continue;
            } else if (std.mem.eql(u8, mode, "minor")) {
                if (tag.version.major != current_version.major) continue;
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

fn shaMatches(actual: []const u8, expected: []const u8) bool {
    return std.mem.eql(u8, actual, expected) or std.mem.startsWith(u8, expected, actual);
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

fn parseVersion(ref: []const u8) ?Version {
    var value = ref;
    if (value.len > 0 and (value[0] == 'v' or value[0] == 'V')) {
        value = value[1..];
    }
    if (value.len == 0 or !std.ascii.isDigit(value[0])) return null;

    var parts = std.mem.splitScalar(u8, value, '.');
    const major_raw = parts.next() orelse return null;
    const minor_raw = parts.next() orelse "0";
    const patch_raw = parts.next() orelse "0";

    return .{
        .major = parseLeadingInt(major_raw) orelse return null,
        .minor = parseLeadingInt(minor_raw) orelse return null,
        .patch = parseLeadingInt(patch_raw) orelse return null,
    };
}

fn parseLeadingInt(value: []const u8) ?u32 {
    var end: usize = 0;
    while (end < value.len and std.ascii.isDigit(value[end])) : (end += 1) {}
    if (end == 0) return null;
    return std.fmt.parseInt(u32, value[0..end], 10) catch null;
}

fn isLikelySha(value: []const u8) bool {
    if (value.len < 7 or value.len > 40) return false;
    for (value) |char| {
        if (!std.ascii.isHex(char)) return false;
    }
    return true;
}

test "choose target respects mode" {
    const tags = [_]Tag{
        .{ .name = "v1.2.3", .sha = "a", .version = .{ .major = 1, .minor = 2, .patch = 3 } },
        .{ .name = "v1.3.0", .sha = "b", .version = .{ .major = 1, .minor = 3, .patch = 0 } },
        .{ .name = "v2.0.0", .sha = "c", .version = .{ .major = 2, .minor = 0, .patch = 0 } },
    };

    try std.testing.expectEqualStrings("v1.2.3", chooseTarget(&tags, .{ .major = 1, .minor = 2, .patch = 0 }, "patch").?.name);
    try std.testing.expectEqualStrings("v1.3.0", chooseTarget(&tags, .{ .major = 1, .minor = 2, .patch = 0 }, "minor").?.name);
    try std.testing.expectEqualStrings("v2.0.0", chooseTarget(&tags, .{ .major = 1, .minor = 2, .patch = 0 }, "major").?.name);
}
