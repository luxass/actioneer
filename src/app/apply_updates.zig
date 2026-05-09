const std = @import("std");

const github = @import("../core/github.zig");
const output = @import("ui/output.zig");
const log = @import("../core/log.zig");
const rewrite = @import("../core/rewrite.zig");

pub const ApplyError = rewrite.RewriteError;

pub const CommandOptions = struct {
    candidates: []const github.Candidate,
    selected: []const usize,
};

pub fn runForCommand(
    allocator: std.mem.Allocator,
    io: std.Io,
    writer: *std.Io.Writer,
    options: CommandOptions,
) !void {
    const applied = run(allocator, io, options.candidates, options.selected) catch |err| {
        log.debug("apply failed error={s} selected={d}", .{ @errorName(err), options.selected.len });
        try output.writeApplyError(writer, err);
        return;
    };

    log.debug("apply complete applied={d}", .{applied});
    try output.writeApplyComplete(writer, applied);
}

pub fn run(
    allocator: std.mem.Allocator,
    io: std.Io,
    candidates: []const github.Candidate,
    selected: []const usize,
) ApplyError!usize {
    return rewrite.rewriteSelectedFiles(allocator, io, candidates, selected);
}
