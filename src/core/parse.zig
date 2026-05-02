const std = @import("std");

const types = @import("types.zig");
const yaml = @import("parse-yaml.zig");

pub fn parseWorkflowString(
    allocator: std.mem.Allocator,
    file_path: []const u8,
    contents: []const u8,
) ![]types.FoundAction {
    var found: std.ArrayList(types.FoundAction) = .empty;
    errdefer {
        for (found.items) |action| deinitFoundAction(allocator, action);
        found.deinit(allocator);
    }

    const document = try yaml.parseYaml(allocator, contents);
    defer document.deinit();

    const initial_scope: []const u8 = if (isCompositeAction(file_path)) "composite" else "workflow";
    try collectActionsFromNode(allocator, document.root, file_path, initial_scope, &found);

    return found.toOwnedSlice(allocator);
}

pub fn deinitFoundAction(allocator: std.mem.Allocator, action: types.FoundAction) void {
    allocator.free(action.action);
    allocator.free(action.owner);
    allocator.free(action.repo);
    allocator.free(action.ref);
    if (action.version_comment.len > 0) allocator.free(action.version_comment);
    allocator.free(action.job);
    allocator.free(action.file);
}

pub fn deinitFoundActions(allocator: std.mem.Allocator, found: []types.FoundAction) void {
    for (found) |action| {
        deinitFoundAction(allocator, action);
    }
    allocator.free(found);
}

fn collectActionsFromNode(
    allocator: std.mem.Allocator,
    node: *const yaml.Node,
    file_path: []const u8,
    scope: []const u8,
    found: *std.ArrayList(types.FoundAction),
) !void {
    switch (node.value) {
        .map => |pairs| {
            for (pairs) |pair| {
                if (std.mem.eql(u8, pair.key, "jobs") and pair.value.value == .map) {
                    for (pair.value.value.map) |job| {
                        try collectActionsFromNode(allocator, job.value, file_path, job.key, found);
                    }
                    continue;
                }

                if (std.mem.eql(u8, pair.key, "uses")) {
                    if (pair.value.scalar()) |value| {
                        if (actionFromUsesValue(allocator, value, pair.value.comment, scope, file_path, pair.value.line)) |action| {
                            try found.append(allocator, action);
                        } else |_| {}
                    }
                    continue;
                }

                try collectActionsFromNode(allocator, pair.value, file_path, scope, found);
            }
        },
        .list => |items| {
            for (items) |item| {
                try collectActionsFromNode(allocator, item, file_path, scope, found);
            }
        },
        .scalar, .null => {},
    }
}

fn actionFromUsesValue(
    allocator: std.mem.Allocator,
    value: []const u8,
    comment: []const u8,
    scope: []const u8,
    file_path: []const u8,
    line: u32,
) !types.FoundAction {
    if (std.mem.startsWith(u8, value, "./") or
        std.mem.startsWith(u8, value, "../") or
        std.mem.startsWith(u8, value, "docker://"))
    {
        return error.InvalidActionReference;
    }

    const parsed = parseActionRef(value) catch return error.InvalidActionReference;
    const version_comment = extractVersionComment(comment);

    return .{
        .action = try allocator.dupe(u8, parsed.action),
        .owner = try allocator.dupe(u8, parsed.owner),
        .repo = try allocator.dupe(u8, parsed.repo),
        .ref = try allocator.dupe(u8, parsed.ref),
        .version_comment = if (version_comment) |version| try allocator.dupe(u8, version) else "",
        .job = try allocator.dupe(u8, scope),
        .file = try allocator.dupe(u8, file_path),
        .line = line,
    };
}

const ParsedActionRef = struct {
    action: []const u8,
    owner: []const u8,
    repo: []const u8,
    ref: []const u8,
};

fn parseActionRef(value: []const u8) !ParsedActionRef {
    const at = std.mem.lastIndexOfScalar(u8, value, '@') orelse return error.InvalidActionReference;
    const action = value[0..at];
    const ref = value[at + 1 ..];

    var parts = std.mem.splitScalar(u8, action, '/');
    const owner = parts.next() orelse return error.InvalidActionReference;
    const repo = parts.next() orelse return error.InvalidActionReference;
    if (owner.len == 0 or repo.len == 0 or ref.len == 0) return error.InvalidActionReference;

    return .{
        .action = action,
        .owner = owner,
        .repo = repo,
        .ref = ref,
    };
}

fn extractVersionComment(comment: []const u8) ?[]const u8 {
    if (comment.len == 0) return null;

    var iter = std.mem.tokenizeAny(u8, comment, " \t,;()[]{}");
    while (iter.next()) |token| {
        const trimmed = std.mem.trim(u8, token, ".:");
        if (parseVersionLike(trimmed)) return trimmed;
    }
    return null;
}

fn parseVersionLike(value: []const u8) bool {
    var rest = value;
    if (rest.len > 0 and (rest[0] == 'v' or rest[0] == 'V')) rest = rest[1..];
    if (rest.len == 0 or !std.ascii.isDigit(rest[0])) return false;

    for (rest) |char| {
        if (char == '.') continue;
        if (!std.ascii.isDigit(char)) return false;
    }
    return true;
}

fn isCompositeAction(path: []const u8) bool {
    return std.mem.endsWith(u8, path, "action.yml") or std.mem.endsWith(u8, path, "action.yaml");
}

test "parse workflow uses" {
    const yamlStr =
        \\name: ci
        \\jobs:
        \\  build:
        \\    steps:
        \\      - uses: actions/checkout@v4
        \\      - name: setup
        \\        uses: pnpm/action-setup@v4.1.0
    ;

    const found = try parseWorkflowString(std.testing.allocator, ".github/workflows/ci.yml", yamlStr);
    defer deinitFoundActions(std.testing.allocator, found);

    try std.testing.expectEqual(@as(usize, 2), found.len);
    try std.testing.expectEqualStrings("build", found[0].job);
    try std.testing.expectEqualStrings("actions/checkout", found[0].action);
    try std.testing.expectEqualStrings("v4", found[0].ref);
}

test "parse version comment on sha pinned action" {
    const yamlStr =
        \\jobs:
        \\  build:
        \\    steps:
        \\      - uses: actions/checkout@0123456789abcdef # v4.1.0
    ;

    const found = try parseWorkflowString(std.testing.allocator, ".github/workflows/ci.yml", yamlStr);
    defer deinitFoundActions(std.testing.allocator, found);

    try std.testing.expectEqual(@as(usize, 1), found.len);
    try std.testing.expectEqualStrings("0123456789abcdef", found[0].ref);
    try std.testing.expectEqualStrings("v4.1.0", found[0].version_comment);
}

test "parse version comments with major-only version" {
    const yamlStr =
        \\jobs:
        \\  build:
        \\    steps:
        \\      - uses: actions/checkout@0123456789abcdef # v4
    ;

    const found = try parseWorkflowString(std.testing.allocator, ".github/workflows/ci.yml", yamlStr);
    defer deinitFoundActions(std.testing.allocator, found);

    try std.testing.expectEqual(@as(usize, 1), found.len);
    try std.testing.expectEqualStrings("actions/checkout", found[0].action);
    try std.testing.expectEqualStrings("0123456789abcdef", found[0].ref);
    try std.testing.expectEqualStrings("v4", found[0].version_comment);
}

test "parse composite action uses" {
    const yamlStr =
        \\runs:
        \\  using: composite
        \\  steps:
        \\    - uses: actions/setup-node@v4 # v4.2.0
        \\    - uses: ./local-action
    ;

    const found = try parseWorkflowString(std.testing.allocator, "action.yml", yamlStr);
    defer deinitFoundActions(std.testing.allocator, found);

    try std.testing.expectEqual(@as(usize, 1), found.len);
    try std.testing.expectEqualStrings("composite", found[0].job);
    try std.testing.expectEqualStrings("actions/setup-node", found[0].action);
    try std.testing.expectEqualStrings("v4", found[0].ref);
    try std.testing.expectEqualStrings("v4.2.0", found[0].version_comment);
}
