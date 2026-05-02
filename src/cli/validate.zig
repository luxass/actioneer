const std = @import("std");
const zli = @import("zli");

const github = @import("../core/github.zig");
const text = @import("../core/ui/text.zig");
const options = @import("options.zig");
const scanner = @import("../core/scanner.zig");
const updates = @import("../core/updates.zig");

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
    const parsed = options.parse(ctx) catch |err| switch (err) {
        error.InvalidOption, error.MissingFlagValue => {
            try ctx.writer.flush();
            std.process.exit(1);
        },
        else => return err,
    };
    defer parsed.deinit(ctx.allocator);

    var arena = std.heap.ArenaAllocator.init(ctx.allocator);
    defer arena.deinit();
    const allocator = arena.allocator();

    const core_config = parsed.core();
    if (!parsed.json) {
        try text.writeScanStart(ctx.writer, parsed.dirs);
    }
    const found = try scanner.scan(allocator, ctx.io, core_config);
    if (found.len == 0) {
        if (parsed.json) {
            const empty: []const updates.Candidate = &.{};
            try text.writeJson(ctx.writer, empty);
            return;
        }

        try text.writeNoReferences(ctx.writer);
        return;
    }

    if (!parsed.json) {
        try text.writeFoundReferences(ctx.writer, found.len);
        try text.writeVerifyStart(ctx.writer, core_config.github_token != null);
    }
    var diagnostics: github.Diagnostics = .{};
    const candidates = github.resolve(allocator, ctx.io, found, core_config, &diagnostics) catch |err| {
        try text.writeResolveError(ctx.writer, err, diagnostics);
        return;
    };
    if (!parsed.json) {
        try text.writeValidationComplete(ctx.writer);
    }

    if (text.hasShaMismatches(candidates)) {
        if (parsed.json) {
            try text.writeJson(ctx.writer, candidates);
        } else {
            try text.writeShaMismatchError(ctx.writer, candidates);
        }
        try ctx.writer.flush();
        std.process.exit(1);
    }

    try text.writeValidationSummary(ctx.writer, found.len, candidates.len);
}
