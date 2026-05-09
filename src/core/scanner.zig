const std = @import("std");

const log = @import("log.zig");
const actions = @import("../syntax/github_actions.zig");

pub const ScanError = error{
    InvalidActionReference,
} || anyerror;

pub fn scan(
    allocator: std.mem.Allocator,
    io: std.Io,
    paths: []const []const u8,
    recursive: bool,
) ScanError![]actions.Reference {
    var found: std.ArrayList(actions.Reference) = .empty;
    errdefer {
        for (found.items) |action| actions.deinitReference(allocator, action);
        found.deinit(allocator);
    }

    for (paths) |path| {
        log.debug("scan path={s} recursive={}", .{
            path,
            recursive or std.mem.eql(u8, path, ".github"),
        });
        scanPath(allocator, io, path, recursive or std.mem.eql(u8, path, ".github"), &found) catch |err| switch (err) {
            error.FileNotFound => {
                log.debug("scan path missing path={s}", .{path});
                continue;
            },
            else => return err,
        };
    }

    return found.toOwnedSlice(allocator);
}

pub fn deinitReferences(allocator: std.mem.Allocator, found: []const actions.Reference) void {
    actions.deinitReferences(allocator, found);
}

fn scanDir(
    allocator: std.mem.Allocator,
    io: std.Io,
    dir_path: []const u8,
    recursive: bool,
    found: *std.ArrayList(actions.Reference),
) ScanError!void {
    var dir = try std.Io.Dir.cwd().openDir(io, dir_path, .{ .iterate = true });
    defer dir.close(io);

    if (recursive) {
        var walker = try dir.walk(allocator);
        defer walker.deinit();

        while (try walker.next(io)) |entry| {
            if (entry.kind != .file) continue;
            if (!isYamlFile(entry.path)) continue;

            const display_path = try std.fmt.allocPrint(allocator, "{s}/{s}", .{ dir_path, entry.path });
            defer allocator.free(display_path);
            const contents = try entry.dir.readFileAlloc(io, entry.basename, allocator, .limited(5 * 1024 * 1024));
            defer allocator.free(contents);

            try appendParsedWorkflow(allocator, display_path, contents, found);
        }
        return;
    }

    var iter = dir.iterate();
    while (try iter.next(io)) |entry| {
        if (entry.kind != .file) continue;
        if (!isYamlFile(entry.name)) continue;

        const display_path = try std.fmt.allocPrint(allocator, "{s}/{s}", .{ dir_path, entry.name });
        defer allocator.free(display_path);
        const contents = try dir.readFileAlloc(io, entry.name, allocator, .limited(5 * 1024 * 1024));
        defer allocator.free(contents);

        try appendParsedWorkflow(allocator, display_path, contents, found);
    }
}

fn scanPath(
    allocator: std.mem.Allocator,
    io: std.Io,
    path: []const u8,
    recursive: bool,
    found: *std.ArrayList(actions.Reference),
) ScanError!void {
    scanDir(allocator, io, path, recursive, found) catch |err| switch (err) {
        error.NotDir => return scanFile(allocator, io, path, found),
        else => return err,
    };
}

fn scanFile(
    allocator: std.mem.Allocator,
    io: std.Io,
    file_path: []const u8,
    found: *std.ArrayList(actions.Reference),
) !void {
    const contents = try std.Io.Dir.cwd().readFileAlloc(io, file_path, allocator, .limited(5 * 1024 * 1024));
    defer allocator.free(contents);

    try appendParsedWorkflow(allocator, file_path, contents, found);
}

fn appendParsedWorkflow(
    allocator: std.mem.Allocator,
    display_path: []const u8,
    contents: []const u8,
    found: *std.ArrayList(actions.Reference),
) !void {
    const parsed = try actions.collectReferences(allocator, display_path, contents);
    defer allocator.free(parsed);

    try found.appendSlice(allocator, parsed);
    log.debug("parsed workflow file={s} bytes={d} actions={d} total_actions={d}", .{
        display_path,
        contents.len,
        parsed.len,
        found.items.len,
    });
}

fn isYamlFile(path: []const u8) bool {
    return std.mem.endsWith(u8, path, ".yml") or std.mem.endsWith(u8, path, ".yaml");
}
