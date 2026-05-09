const std = @import("std");

const github = @import("../core/github.zig");
const scanner = @import("../core/scanner.zig");

pub const Result = struct {
    reference_count: usize,
    candidates: []const github.Candidate,
};

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
