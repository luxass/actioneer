const std = @import("std");
const zli = @import("zli");

const check = @import("../check.zig");
const log = @import("../../core/log.zig");
const runtime = @import("../../core/runtime.zig");
const types = @import("../../core/types.zig");
const options = @import("../options.zig");
const text = @import("../ui.zig");

pub fn register(init_options: zli.InitOptions) !*zli.Command {
    const cmd = try zli.Command.init(init_options, .{
        .name = "validate",
        .description = "Validate action update configuration",
        .short_description = "Validate action update configuration",
    }, run);

    try options.addFlags(cmd);

    return cmd;
}

pub fn run(ctx: zli.CommandContext) !void {
    var prepared = try check.prepare(ctx);
    defer prepared.deinit(ctx.allocator);
    const parsed = prepared.parsed;
    const allocator = prepared.allocator();
    log.debug("command=validate dirs={d} recursive={} mode={s} style={s} json={} ci={} excludes={d}", .{
        parsed.dirs.len,
        parsed.recursive,
        @tagName(parsed.mode),
        @tagName(parsed.style),
        parsed.json,
        runtime.isCi(),
        parsed.excludes.len,
    });

    const result = (try check.run(ctx, allocator, parsed)) orelse return;

    if (!parsed.json) {
        try ctx.writer.print("{s}Verification completed.{s}\n", .{ text.styles.GREEN, text.styles.RESET });
    }

    if (text.hasShaMismatches(result.candidates)) {
        if (parsed.json) {
            try text.writeJson(ctx.writer, result.candidates);
        } else {
            try text.writeShaMismatchError(ctx.writer, result.candidates);
        }
        try ctx.writer.flush();
        return error.CommandFailed;
    }

    if (parsed.json) {
        try text.writeJson(ctx.writer, result.candidates);
        return;
    }

    try text.writeValidationSummary(ctx.writer, result.reference_count, result.candidates.len);
}
