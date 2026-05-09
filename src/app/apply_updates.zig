const std = @import("std");

const file_rewriter = @import("../core/file_rewriter.zig");
const github = @import("../core/github.zig");

pub const ApplyError = file_rewriter.RewriteError;

pub fn run(
    allocator: std.mem.Allocator,
    io: std.Io,
    candidates: []const github.Candidate,
    selected: []const usize,
) ApplyError!usize {
    return file_rewriter.rewriteSelectedFiles(allocator, io, candidates, selected);
}
