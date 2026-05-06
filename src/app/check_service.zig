const std = @import("std");

const github = @import("../core/github.zig");
const scanner = @import("../core/scanner.zig");
const types = @import("../core/types.zig");

pub fn run(
    allocator: std.mem.Allocator,
    io: std.Io,
    options: types.ScanOptions,
    resolve_options: types.ResolveOptions,
    diagnostics: ?*github.Diagnostics,
) !struct {
    reference_count: usize,
    candidates: []const types.Candidate,
} {
    const found = try scanner.scan(allocator, io, options);
    defer scanner.deinitFoundActions(allocator, found);

    var github_client = github.Client.init(allocator, io);
    defer github_client.deinit();

    const candidates = try github_client.resolve(found, resolve_options, diagnostics);
    return .{
        .reference_count = found.len,
        .candidates = candidates,
    };
}
