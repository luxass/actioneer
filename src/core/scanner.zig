const std = @import("std");

const config = @import("config.zig");
const parse = @import("parse.zig");
const types = @import("types.zig");
const updates = @import("updates.zig");

pub const ScanError = error{
    InvalidActionReference,
} || anyerror;

pub fn scan(
    allocator: std.mem.Allocator,
    io: std.Io,
    parsed: config.Config,
) ScanError![]types.FoundAction {
    var found: std.ArrayList(types.FoundAction) = .empty;
    errdefer {
        for (found.items) |action| parse.deinitFoundAction(allocator, action);
        found.deinit(allocator);
    }

    for (parsed.dirs) |path| {
        scanPath(allocator, io, path, parsed.recursive or std.mem.eql(u8, path, ".github"), &found) catch |err| switch (err) {
            error.FileNotFound => continue,
            else => return err,
        };
    }

    return found.toOwnedSlice(allocator);
}

pub fn toUnresolvedCandidates(
    allocator: std.mem.Allocator,
    found: []const types.FoundAction,
) ![]updates.Candidate {
    var candidates: std.ArrayList(updates.Candidate) = .empty;
    errdefer candidates.deinit(allocator);

    for (found) |action| {
        try candidates.append(allocator, .{
            .action = action.action,
            .job = action.job,
            .current = action.ref,
            .version_comment = action.version_comment,
            .next = action.ref,
            .next_label = action.ref,
            .file = action.file,
            .line = action.line,
        });
    }

    return candidates.toOwnedSlice(allocator);
}

fn scanDir(
    allocator: std.mem.Allocator,
    io: std.Io,
    dir_path: []const u8,
    recursive: bool,
    found: *std.ArrayList(types.FoundAction),
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
    found: *std.ArrayList(types.FoundAction),
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
    found: *std.ArrayList(types.FoundAction),
) !void {
    const contents = try std.Io.Dir.cwd().readFileAlloc(io, file_path, allocator, .limited(5 * 1024 * 1024));
    defer allocator.free(contents);

    try appendParsedWorkflow(allocator, file_path, contents, found);
}

fn appendParsedWorkflow(
    allocator: std.mem.Allocator,
    display_path: []const u8,
    contents: []const u8,
    found: *std.ArrayList(types.FoundAction),
) !void {
    const parsed = try parse.parseWorkflowString(allocator, display_path, contents);
    defer allocator.free(parsed);

    try found.appendSlice(allocator, parsed);
}

fn isYamlFile(path: []const u8) bool {
    return std.mem.endsWith(u8, path, ".yml") or std.mem.endsWith(u8, path, ".yaml");
}
