const std = @import("std");

pub const UpdateMode = enum {
    major,
    minor,
    patch,
};

pub const PinStyle = enum {
    sha,
    preserve,
};

pub const CheckOptions = struct {
    dirs: []const []const u8,
    recursive: bool,
    excludes: []const []const u8,
    include_branches: bool,
    mode: UpdateMode,
    style: PinStyle,
};

pub const CheckResult = struct {
    reference_count: usize,
    candidates: []const Candidate,
};

pub const ReferenceKind = enum {
    workflow_job,
    workflow_step,
    composite_step,
};

pub const ByteSpan = struct {
    start: u32,
    end: u32,
};

pub const SourceLocation = struct {
    file: []const u8,
    line: u32,
    ref_span: ByteSpan,
};

pub const Reference = struct {
    kind: ReferenceKind,
    name: ActionName,
    current_ref: []const u8,
    version_hint: []const u8 = "",
    scope: []const u8,
    source: SourceLocation,
};

pub const Repository = struct {
    owner: []const u8,
    name: []const u8,

    pub fn allocDisplay(self: Repository, allocator: anytype) ![]const u8 {
        return std.fmt.allocPrint(allocator, "{s}/{s}", .{ self.owner, self.name });
    }
};

pub const ActionName = struct {
    repository: Repository,
    path: []const u8 = "",

    pub fn displayLen(self: ActionName) usize {
        return self.repository.owner.len + 1 + self.repository.name.len + self.path.len;
    }

    pub fn allocDisplay(self: ActionName, allocator: anytype) ![]const u8 {
        return std.fmt.allocPrint(allocator, "{s}/{s}{s}", .{ self.repository.owner, self.repository.name, self.path });
    }

    pub fn eqlDisplay(self: ActionName, value: []const u8) bool {
        if (value.len != self.displayLen()) return false;
        if (!std.mem.startsWith(u8, value, self.repository.owner)) return false;
        if (value[self.repository.owner.len] != '/') return false;

        const name_start = self.repository.owner.len + 1;
        if (!std.mem.startsWith(u8, value[name_start..], self.repository.name)) return false;
        return std.mem.eql(u8, value[name_start + self.repository.name.len ..], self.path);
    }
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
