const std = @import("std");

const file_rewriter = @import("../core/file_rewriter.zig");
const types = @import("../core/types.zig");

pub const ApplyError = file_rewriter.RewriteError;

pub fn applySelected(
    allocator: std.mem.Allocator,
    io: std.Io,
    candidates: []const types.Candidate,
    selected: []const usize,
) ApplyError!usize {
    return file_rewriter.rewriteSelectedFiles(allocator, io, candidates, selected);
}
