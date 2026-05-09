const std = @import("std");
const zli = @import("zli");

const apply_updates = @import("../app/apply_updates.zig");
const prompt = @import("../app/ui/prompt.zig");
const output = @import("../app/ui/output.zig");
const cli = @import("../cli.zig");
const log = @import("../core/log.zig");

pub fn run(ctx: zli.CommandContext) !void {
    const input = try cli.parseCommandInput(ctx, .update);
    defer input.deinit(ctx.allocator);

    cli.initRuntime(input);

    var arena = std.heap.ArenaAllocator.init(ctx.allocator);
    defer arena.deinit();

    log.debug("command=update dirs={d} recursive={} mode={s} style={s} dry_run={} yes={} json={} ci={} excludes={d}", .{
        input.paths.len,
        input.recursive,
        @tagName(input.mode),
        @tagName(input.style),
        input.dry_run,
        input.yes,
        input.json,
        input.ci,
        input.excludes.len,
    });

    const result = (try cli.runCheck(arena.allocator(), ctx, input)) orelse return;

    if (input.wantsHumanOutput()) {
        try output.writeUpdateCount(ctx.writer, result.candidates.len);
        try output.writeShaMismatchWarning(ctx.writer, result.candidates);
    }

    if (input.wantsJsonOutput()) {
        try output.writeJson(ctx.writer, result.candidates);
        return;
    }

    if (input.wantsPreview()) {
        try output.writePreview(ctx.writer, result.reference_count, result.candidates);
        return;
    }

    if (result.candidates.len == 0) {
        try ctx.writer.print("{s}Everything is already up to date.{s}\n", .{ output.styles.GREEN, output.styles.RESET });
        return;
    }

    const selected = if (input.shouldAutoSelectAll())
        try selectAll(ctx.allocator, result.candidates.len)
    else
        prompt.selectUpdates(ctx.allocator, ctx, result.candidates) catch |err| switch (err) {
            error.NotATerminal => {
                try output.writeSelectionUnavailable(ctx.writer, .not_tty);
                return;
            },
            error.UnsupportedPlatform => {
                try output.writeSelectionUnavailable(ctx.writer, .unsupported);
                return;
            },
            error.Canceled => {
                try output.writeSelectionCanceled(ctx.writer, false);
                return;
            },
            error.Interrupted => {
                try output.writeSelectionCanceled(ctx.writer, true);
                return;
            },
            else => return err,
        };
    defer ctx.allocator.free(selected);

    if (selected.len == 0) {
        try ctx.writer.print("{s}No updates selected.{s} No files were changed.\n", .{ output.styles.YELLOW, output.styles.RESET });
        return;
    }

    try output.writeSelectedUpdates(ctx.writer, result.candidates, selected);

    const applied = apply_updates.run(arena.allocator(), ctx.io, result.candidates, selected) catch |err| {
        log.debug("apply failed error={s} selected={d}", .{ @errorName(err), selected.len });
        try output.writeApplyError(ctx.writer, err);
        return;
    };

    log.debug("apply complete applied={d}", .{applied});
    try output.writeApplyComplete(ctx.writer, applied);
}

fn selectAll(allocator: std.mem.Allocator, count: usize) ![]usize {
    const selected = try allocator.alloc(usize, count);
    for (selected, 0..) |*item, index| item.* = index;
    return selected;
}
