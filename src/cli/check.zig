const std = @import("std");
const zli = @import("zli");

const config = @import("../app/config.zig");
const check_service = @import("../app/check_service.zig");
const github = @import("../core/github.zig");
const log = @import("../core/log.zig");
const runtime = @import("../core/runtime.zig");
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
    runtime.init(config.AppConfig.fromInputs(ctx.getContextData(options.AppContext).environ_map, parsed.verbose));

    if (!parsed.json) {
        try text.writeScanStart(ctx.writer, parsed.dirs);
    }

    var diagnostics: github.Diagnostics = .{};
    const result = check_service.run(allocator, ctx.io, parsed.scanOptions(), parsed.resolveOptions(), &diagnostics) catch |err| {
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
        result.reference_count,
        result.candidates.len,
        text.shaMismatchCount(result.candidates),
    });

    if (result.reference_count == 0) {
        if (parsed.json) {
            const empty: []const types.Candidate = &.{};
            try text.writeJson(ctx.writer, empty);
            return null;
        }

        try text.writeNoReferences(ctx.writer);
        return null;
    }

    if (!parsed.json) {
        try text.writeFoundReferences(ctx.writer, result.reference_count);
    }

    return .{
        .reference_count = result.reference_count,
        .candidates = result.candidates,
    };
}
