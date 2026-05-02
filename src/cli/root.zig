const std = @import("std");
const zli = @import("zli");

const apply = @import("../core/apply.zig");
const github = @import("../core/github.zig");
const text = @import("../core/ui/text.zig");
const options = @import("options.zig");
const prompt = @import("prompt.zig");
const scanner = @import("../core/scanner.zig");
const updates = @import("../core/updates.zig");
const validate = @import("validate.zig");
const version = @import("version.zig");

pub fn build(init_options: zli.InitOptions) !*zli.Command {
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
        try text.writeUpdateCount(ctx.writer, candidates.len);
        try text.writeShaMismatchWarning(ctx.writer, candidates);
    }

    if (parsed.json) {
        try text.writeJson(ctx.writer, candidates);
        return;
    }

    if (parsed.dry_run) {
        try text.writePreview(ctx.writer, found.len, candidates);
        return;
    }

    if (candidates.len == 0) {
        try text.writeNoUpdates(ctx.writer);
        return;
    }

    const selected = if (parsed.yes)
        try selectAll(ctx.allocator, candidates.len)
    else
        prompt.selectUpdates(ctx.allocator, ctx, candidates) catch |err| switch (err) {
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
        try text.writeNoSelection(ctx.writer);
        return;
    }

    try text.writeSelectedUpdates(ctx.writer, candidates, selected);

    const applied = apply.applySelected(allocator, ctx.io, candidates, selected) catch |err| {
        try text.writeApplyError(ctx.writer, err);
        return;
    };

    try text.writeApplyComplete(ctx.writer, applied);
}

fn selectAll(allocator: std.mem.Allocator, count: usize) ![]usize {
    const selected = try allocator.alloc(usize, count);
    for (selected, 0..) |*item, index| item.* = index;
    return selected;
}
