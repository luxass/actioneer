const std = @import("std");
const zli = @import("zli");

const github = @import("../core/github.zig");
const log = @import("../core/log.zig");
const scanner = @import("../core/scanner.zig");
const types = @import("../core/types.zig");
const options = @import("options.zig");
const text = @import("ui.zig");

pub const Prepared = struct {
    parsed: options.Parsed,
    arena: std.heap.ArenaAllocator,

    pub fn allocator(self: *Prepared) std.mem.Allocator {
        return self.arena.allocator();
    }

    pub fn deinit(self: *Prepared, parent_allocator: std.mem.Allocator) void {
        self.parsed.deinit(parent_allocator);
        self.arena.deinit();
    }
};

pub fn prepare(ctx: zli.CommandContext) !Prepared {
    const parsed = options.parse(ctx) catch |err| switch (err) {
        error.InvalidOption, error.MissingFlagValue => {
            try ctx.writer.flush();
            return error.CommandFailed;
        },
        else => return err,
    };

    return .{
        .parsed = parsed,
        .arena = std.heap.ArenaAllocator.init(ctx.allocator),
    };
}

pub fn run(
    ctx: zli.CommandContext,
    allocator: std.mem.Allocator,
    parsed: options.Parsed,
) !?types.CheckResult {
    if (!parsed.json) {
        try text.writeScanStart(ctx.writer, parsed.dirs);
    }

    var diagnostics: github.Diagnostics = .{};
    const found = scanner.scan(allocator, ctx.io, parsed.scanOptions()) catch |err| {
        log.debug("check failed error={s} repository={s} status={?} cause={s}", .{
            @errorName(err),
            diagnostics.repository,
            diagnostics.status,
            diagnostics.cause,
        });
        try text.writeCheckError(ctx.writer, err, diagnostics);
        return null;
    };
    defer scanner.deinitFoundActions(allocator, found);

    var github_client = github.Client.init(allocator, ctx.io);
    defer github_client.deinit();

    const result = github_client.resolve(found, parsed.resolveOptions(), &diagnostics) catch |err| {
        log.debug("check failed error={s} repository={s} status={?} cause={s}", .{
            @errorName(err),
            diagnostics.repository,
            diagnostics.status,
            diagnostics.cause,
        });
        try text.writeCheckError(ctx.writer, err, diagnostics);
        return null;
    };

    log.debug("check complete found_actions={d} candidates={d} sha_mismatches={d}", .{
        found.len,
        result.len,
        text.shaMismatchCount(result),
    });

    if (found.len == 0) {
        if (parsed.json) {
            const empty: []const types.Candidate = &.{};
            try text.writeJson(ctx.writer, empty);
            return null;
        }

        try text.writeNoReferences(ctx.writer);
        return null;
    }

    if (!parsed.json) {
        try text.writeFoundReferences(ctx.writer, found.len);
    }

    return .{
        .reference_count = found.len,
        .candidates = result,
    };
}
