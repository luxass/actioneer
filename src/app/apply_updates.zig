const std = @import("std");

const rewrite = @import("../core/rewrite.zig");
const github = @import("../core/github.zig");

pub const ApplyError = rewrite.RewriteError;

pub fn run(
    allocator: std.mem.Allocator,
    io: std.Io,
    candidates: []const github.Candidate,
    selected: []const usize,
) ApplyError!usize {
    return rewrite.rewriteSelectedFiles(allocator, io, candidates, selected);
}
