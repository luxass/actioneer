const std = @import("std");

const output = @import("ui/output.zig");
const github = @import("../core/github.zig");
const log = @import("../core/log.zig");
const scanner = @import("../core/scanner.zig");

pub const Result = struct {
    reference_count: usize,
    candidates: []const github.Candidate,
};

pub const CommandOptions = struct {
    paths: []const []const u8,
    recursive: bool,
    resolve_options: github.ResolveOptions,
    human_output: bool,
    json_output: bool,
};

pub fn runForCommand(
    allocator: std.mem.Allocator,
    io: std.Io,
    writer: *std.Io.Writer,
    options: CommandOptions,
) !?Result {
    if (options.human_output) {
        try output.writeScanStart(writer, options.paths);
    }

    var diagnostics: github.Diagnostics = .{};
    const result = run(
        allocator,
        io,
        options.paths,
        options.recursive,
        options.resolve_options,
        &diagnostics,
    ) catch |err| {
        log.debug("check failed error={s} repository={s} status={?} cause={s}", .{
            @errorName(err),
            diagnostics.repository,
            diagnostics.status,
            diagnostics.cause,
        });
        try output.writeCheckError(writer, err, diagnostics);
        return null;
    };

    log.debug("check complete found_actions={d} candidates={d} sha_mismatches={d}", .{
        result.reference_count,
        result.candidates.len,
        output.shaMismatchCount(result.candidates),
    });

    if (result.reference_count == 0) {
        if (options.json_output) {
            const empty: []const github.Candidate = &.{};
            try output.writeJson(writer, empty);
            return null;
        }

        try output.writeNoReferences(writer);
        return null;
    }

    if (options.human_output) {
        try output.writeFoundReferences(writer, result.reference_count);
    }

    return result;
}

pub fn run(
    allocator: std.mem.Allocator,
    io: std.Io,
    paths: []const []const u8,
    recursive: bool,
    resolve_options: github.ResolveOptions,
    diagnostics: ?*github.Diagnostics,
) !Result {
    const found = try scanner.scan(allocator, io, paths, recursive);
    defer scanner.deinitReferences(allocator, found);

    var github_client = github.Client.init(allocator, io);
    defer github_client.deinit();

    const candidates = try github_client.resolve(found, resolve_options, diagnostics);
    return .{
        .reference_count = found.len,
        .candidates = candidates,
    };
}
