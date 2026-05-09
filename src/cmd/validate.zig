const std = @import("std");
const zli = @import("zli");

const cli = @import("../cli.zig");
const output = @import("../app/ui/output.zig");
const log = @import("../core/log.zig");

pub fn register(init_options: zli.InitOptions) !*zli.Command {
    const cmd = try zli.Command.init(init_options, .{
        .name = "validate",
        .description = "Validate action update configuration",
        .short_description = "Validate action update configuration",
    }, run);

    try cli.addFlags(cmd);

    return cmd;
}

pub fn run(ctx: zli.CommandContext) !void {
    const input = try cli.parseCommandInput(ctx, .validate);
    defer input.deinit(ctx.allocator);

    cli.initRuntime(input);

    var arena = std.heap.ArenaAllocator.init(ctx.allocator);
    defer arena.deinit();

    log.debug("command=validate dirs={d} recursive={} mode={s} style={s} json={} ci={} excludes={d}", .{
        input.paths.len,
        input.recursive,
        @tagName(input.mode),
        @tagName(input.style),
        input.json,
        input.ci,
        input.excludes.len,
    });

    const result = (try cli.runCheck(arena.allocator(), ctx, input)) orelse return;

    if (input.wantsHumanOutput()) {
        try ctx.writer.print("{s}Verification completed.{s}\n", .{ output.styles.GREEN, output.styles.RESET });
    }

    if (output.hasShaMismatches(result.candidates)) {
        if (input.wantsJsonOutput()) {
            try output.writeJson(ctx.writer, result.candidates);
        } else {
            try output.writeShaMismatchError(ctx.writer, result.candidates);
        }
        try ctx.writer.flush();
        return error.CommandFailed;
    }

    if (input.wantsJsonOutput()) {
        try output.writeJson(ctx.writer, result.candidates);
        return;
    }

    try output.writeValidationSummary(ctx.writer, result.reference_count, result.candidates.len);
}
