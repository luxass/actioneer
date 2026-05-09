const std = @import("std");

const github = @import("../core/github.zig");
const scanner = @import("../core/scanner.zig");
const types = @import("../core/types.zig");

pub fn run(
    allocator: std.mem.Allocator,
    io: std.Io,
    options: types.CheckOptions,
    diagnostics: ?*github.Diagnostics,
) !types.CheckResult {
    const found = try scanner.scan(allocator, io, options);
    defer scanner.deinitReferences(allocator, found);

    var github_client = github.Client.init(allocator, io);
    defer github_client.deinit();

    const candidates = try github_client.resolve(found, options, diagnostics);
    return .{
        .reference_count = found.len,
        .candidates = candidates,
    };
}
