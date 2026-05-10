const std = @import("std");
const zli = @import("zli");

const apply_updates = @import("../app/apply_updates.zig");
const check_workflows = @import("../app/check_workflows.zig");
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

    const result = (try check_workflows.runForCommand(arena.allocator(), ctx.io, ctx.writer, .{
        .paths = input.paths,
        .recursive = input.recursive,
        .resolve_options = input.resolveOptions(),
        .human_output = input.wantsHumanOutput(),
        .json_output = input.wantsJsonOutput(),
    })) orelse return;

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

    try apply_updates.runForCommand(arena.allocator(), ctx.io, ctx.writer, .{
        .candidates = result.candidates,
        .selected = selected,
    });
}

fn selectAll(allocator: std.mem.Allocator, count: usize) ![]usize {
    const selected = try allocator.alloc(usize, count);
    for (selected, 0..) |*item, index| item.* = index;
    return selected;
}
