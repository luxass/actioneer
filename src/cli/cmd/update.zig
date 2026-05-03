const std = @import("std");
const zli = @import("zli");

const apply = @import("../../core/apply.zig");
const github = @import("../../core/github.zig");
const check = @import("../check.zig");
const log = @import("../../core/log.zig");
const runtime = @import("../../core/runtime.zig");
const types = @import("../../core/types.zig");
const options = @import("../options.zig");
const prompt = @import("../prompt.zig");
const text = @import("../ui.zig");
const validate = @import("validate.zig");
const version = @import("version.zig");

pub fn register(init_options: zli.InitOptions) !*zli.Command {
    const root = try zli.Command.init(init_options, .{
        .name = "actioneer",
        .description = "Actioneer CLI",
        .version = .{ .major = 0, .minor = 0, .patch = 0 },
    }, run);

    try options.addFlags(root);

    try root.addCommands(&.{
        try validate.register(init_options),
        try version.register(init_options),
    });

    return root;
}

fn run(ctx: zli.CommandContext) !void {
    var prepared = try check.prepare(ctx);
    defer prepared.deinit(ctx.allocator);
    const parsed = prepared.parsed;
    const allocator = prepared.allocator();
    const app_context = ctx.getContextData(options.AppContext);
    const github_token = github.tokenFromEnv(app_context.environ_map);

    log.debug("command=update dirs={d} recursive={} mode={s} style={s} dry_run={} yes={} json={} ci={} excludes={d} github_token={}", .{
        parsed.dirs.len,
        parsed.recursive,
        @tagName(parsed.mode),
        @tagName(parsed.style),
        parsed.dry_run,
        parsed.yes,
        parsed.json,
        runtime.isCi(),
        parsed.excludes.len,
        github_token != null,
    });

    const result = (try check.run(ctx, allocator, parsed)) orelse return;

    if (!parsed.json) {
        try text.writeUpdateCount(ctx.writer, result.candidates.len);
        try text.writeShaMismatchWarning(ctx.writer, result.candidates);
    }

    if (parsed.json) {
        try text.writeJson(ctx.writer, result.candidates);
        return;
    }

    if (parsed.dry_run) {
        try text.writePreview(ctx.writer, result.reference_count, result.candidates);
        return;
    }

    if (result.candidates.len == 0) {
        try ctx.writer.print("{s}Everything is already up to date.{s}\n", .{ text.styles.GREEN, text.styles.RESET });
        return;
    }

    const selected = if (parsed.yes)
        try selectAll(ctx.allocator, result.candidates.len)
    else
        prompt.selectUpdates(ctx.allocator, ctx, result.candidates) catch |err| switch (err) {
            error.NotATerminal => {
                try text.writeSelectionUnavailable(ctx.writer, .not_tty);
                return;
            },
            error.UnsupportedPlatform => {
                try text.writeSelectionUnavailable(ctx.writer, .unsupported);
                return;
            },
            error.Canceled => {
                try text.writeSelectionCanceled(ctx.writer, false);
                return;
            },
            error.Interrupted => {
                try text.writeSelectionCanceled(ctx.writer, true);
                return;
            },
            else => return err,
        };
    defer ctx.allocator.free(selected);

    if (selected.len == 0) {
        try ctx.writer.print("{s}No updates selected.{s} No files were changed.\n", .{ text.styles.YELLOW, text.styles.RESET });
        return;
    }

    try text.writeSelectedUpdates(ctx.writer, result.candidates, selected);

    const applied = apply.applySelected(allocator, ctx.io, result.candidates, selected) catch |err| {
        log.debug("apply failed error={s} selected={d}", .{ @errorName(err), selected.len });
        try text.writeApplyError(ctx.writer, err);
        return;
    };

    log.debug("apply complete applied={d}", .{applied});
    try text.writeApplyComplete(ctx.writer, applied);
}

fn selectAll(allocator: std.mem.Allocator, count: usize) ![]usize {
    const selected = try allocator.alloc(usize, count);
    for (selected, 0..) |*item, index| item.* = index;
    return selected;
}
