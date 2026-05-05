const std = @import("std");
const ts = @import("tree-sitter");

const types = @import("types.zig");

extern fn tree_sitter_yaml() callconv(.c) *const ts.Language;

const ScalarKinds = std.StaticStringMap(void).initComptime(.{
    .{ "alias_name", {} },
    .{ "anchor_name", {} },
    .{ "boolean_scalar", {} },
    .{ "double_quote_scalar", {} },
    .{ "float_scalar", {} },
    .{ "integer_scalar", {} },
    .{ "null_scalar", {} },
    .{ "single_quote_scalar", {} },
    .{ "string_scalar", {} },
    .{ "timestamp_scalar", {} },
});

const WrapperKinds = std.StaticStringMap(void).initComptime(.{
    .{ "block_node", {} },
    .{ "document", {} },
    .{ "flow_node", {} },
    .{ "plain_scalar", {} },
    .{ "stream", {} },
});

const MappingKinds = std.StaticStringMap(void).initComptime(.{
    .{ "block_mapping", {} },
    .{ "flow_mapping", {} },
});

const PairKinds = std.StaticStringMap(void).initComptime(.{
    .{ "block_mapping_pair", {} },
    .{ "flow_pair", {} },
});

const ScalarRange = struct {
    text: []const u8,
    start_byte: u32,
    end_byte: u32,
};

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

    const parser = ts.Parser.create();
    defer parser.destroy();
    try parser.setLanguage(tree_sitter_yaml());

    const tree = parser.parseString(contents, null) orelse return error.InvalidYaml;
    defer tree.destroy();

    const initial_scope: []const u8 = if (isCompositeAction(file_path)) "composite" else "workflow";
    try collectActionsFromNode(allocator, contents, tree.rootNode(), file_path, initial_scope, &found);

    return found.toOwnedSlice(allocator);
}

pub fn deinitFoundAction(allocator: std.mem.Allocator, action: types.FoundAction) void {
    allocator.free(action.action.repository.owner);
    allocator.free(action.action.repository.name);
    if (action.action.path.len > 0) allocator.free(action.action.path);
    allocator.free(action.ref);
    if (action.version_comment.len > 0) allocator.free(action.version_comment);
    allocator.free(action.job);
    allocator.free(action.file);
}

pub fn deinitFoundActions(allocator: std.mem.Allocator, found: []const types.FoundAction) void {
    for (found) |action| deinitFoundAction(allocator, action);
    allocator.free(found);
}

fn collectActionsFromNode(
    allocator: std.mem.Allocator,
    contents: []const u8,
    node: ts.Node,
    file_path: []const u8,
    scope: []const u8,
    found: *std.ArrayList(types.FoundAction),
) anyerror!void {
    const kind = node.kind();

    if (PairKinds.has(kind)) {
        try collectFromPair(allocator, contents, node, file_path, scope, found);
        return;
    }

    var index: u32 = 0;
    while (index < node.namedChildCount()) : (index += 1) {
        try collectActionsFromNode(allocator, contents, node.namedChild(index).?, file_path, scope, found);
    }
}

fn collectFromPair(
    allocator: std.mem.Allocator,
    contents: []const u8,
    pair: ts.Node,
    file_path: []const u8,
    scope: []const u8,
    found: *std.ArrayList(types.FoundAction),
) anyerror!void {
    const key_node = pair.childByFieldName("key") orelse return;
    const key_range = scalarRange(contents, key_node) orelse return;
    const key = cleanScalar(key_range.text);

    const value_node = pair.childByFieldName("value") orelse return;

    if (std.mem.eql(u8, key, "jobs")) {
        try collectJobs(allocator, contents, value_node, file_path, found);
        return;
    }

    if (std.mem.eql(u8, key, "uses")) {
        const value_range = scalarRange(contents, value_node) orelse return;
        if (actionFromUsesValue(
            allocator,
            value_range.text,
            extractTrailingComment(contents, value_node),
            scope,
            file_path,
            value_node.startPoint().row + 1,
            value_range.start_byte,
            value_range.end_byte,
        )) |action| {
            try found.append(allocator, action);
        } else |_| {}
        return;
    }

    try collectActionsFromNode(allocator, contents, value_node, file_path, scope, found);
}

fn collectJobs(
    allocator: std.mem.Allocator,
    contents: []const u8,
    node: ts.Node,
    file_path: []const u8,
    found: *std.ArrayList(types.FoundAction),
) anyerror!void {
    const kind = node.kind();

    if (PairKinds.has(kind)) {
        const key_node = node.childByFieldName("key") orelse return;
        const key_range = scalarRange(contents, key_node) orelse return;
        const job_scope = cleanScalar(key_range.text);
        const value_node = node.childByFieldName("value") orelse return;
        try collectActionsFromNode(allocator, contents, value_node, file_path, job_scope, found);
        return;
    }

    if (WrapperKinds.has(kind) or MappingKinds.has(kind)) {
        var index: u32 = 0;
        while (index < node.namedChildCount()) : (index += 1) {
            try collectJobs(allocator, contents, node.namedChild(index).?, file_path, found);
        }
    }
}

fn scalarRange(contents: []const u8, node: ts.Node) ?ScalarRange {
    const kind = node.kind();

    if (WrapperKinds.has(kind)) {
        if (node.namedChildCount() == 0) return null;
        return scalarRange(contents, node.namedChild(0).?);
    }

    if (!ScalarKinds.has(kind)) return null;

    const raw_start = node.startByte();
    const raw_end = node.endByte();
    const raw = contents[raw_start..raw_end];
    const trimmed = cleanScalar(raw);
    const leading = @as(u32, @intCast(std.mem.indexOf(u8, raw, trimmed) orelse 0));

    return .{
        .text = trimmed,
        .start_byte = raw_start + leading,
        .end_byte = raw_start + leading + @as(u32, @intCast(trimmed.len)),
    };
}

fn extractTrailingComment(contents: []const u8, value_node: ts.Node) []const u8 {
    const end = value_node.endByte();
    const line_end = std.mem.indexOfScalarPos(u8, contents, end, '\n') orelse contents.len;
    const tail = contents[end..line_end];
    const comment_start = std.mem.indexOfScalar(u8, tail, '#') orelse return "";
    return std.mem.trim(u8, tail[comment_start + 1 ..], " \t");
}

fn actionFromUsesValue(
    allocator: std.mem.Allocator,
    value: []const u8,
    comment: []const u8,
    scope: []const u8,
    file_path: []const u8,
    line: u32,
    value_start: u32,
    value_end: u32,
) !types.FoundAction {
    if (std.mem.startsWith(u8, value, "./") or
        std.mem.startsWith(u8, value, "../") or
        std.mem.startsWith(u8, value, "docker://"))
    {
        return error.InvalidActionReference;
    }

    const parsed = parseActionRef(value) catch return error.InvalidActionReference;
    const version_comment = extractVersionComment(comment);
    const ref_start = value_start + @as(u32, @intCast(parsed.action.len + 1));

    return .{
        .action = .{
            .repository = .{
                .owner = try allocator.dupe(u8, parsed.owner),
                .name = try allocator.dupe(u8, parsed.name),
            },
            .path = if (parsed.path.len > 0) try allocator.dupe(u8, parsed.path) else "",
        },
        .ref = try allocator.dupe(u8, parsed.ref),
        .version_comment = if (version_comment) |version| try allocator.dupe(u8, version) else "",
        .job = try allocator.dupe(u8, scope),
        .file = try allocator.dupe(u8, file_path),
        .line = line,
        .ref_start = ref_start,
        .ref_end = value_end,
    };
}

const ParsedActionRef = struct {
    action: []const u8,
    owner: []const u8,
    name: []const u8,
    path: []const u8 = "",
    ref: []const u8,
};

fn parseActionRef(value: []const u8) !ParsedActionRef {
    const at = std.mem.lastIndexOfScalar(u8, value, '@') orelse return error.InvalidActionReference;
    const action = value[0..at];
    const ref = value[at + 1 ..];

    var parts = std.mem.splitScalar(u8, action, '/');
    const owner = parts.next() orelse return error.InvalidActionReference;
    const name = parts.next() orelse return error.InvalidActionReference;
    if (owner.len == 0 or name.len == 0 or ref.len == 0) return error.InvalidActionReference;
    const path_start = owner.len + 1 + name.len;

    return .{
        .action = action,
        .owner = owner,
        .name = name,
        .path = if (path_start < action.len) action[path_start..] else "",
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

fn cleanScalar(value: []const u8) []const u8 {
    var result = std.mem.trim(u8, value, " \t");
    if (result.len >= 2) {
        const first = result[0];
        const last = result[result.len - 1];
        if ((first == '"' and last == '"') or (first == '\'' and last == '\'')) {
            result = result[1 .. result.len - 1];
        }
    }
    return result;
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
    const action_name = try found[0].action.allocDisplay(std.testing.allocator);
    defer std.testing.allocator.free(action_name);
    try std.testing.expectEqualStrings("actions/checkout", action_name);
    try std.testing.expectEqualStrings("v4", found[0].ref);
}

test "parse reusable workflow uses" {
    const yamlStr =
        \\name: security
        \\jobs:
        \\  zizmor:
        \\    uses: luxass/shared-workflows/.github/workflows/reusable-ci-security.yaml@v0.6.0
    ;

    const found = try parseWorkflowString(std.testing.allocator, ".github/workflows/ci-security.yml", yamlStr);
    defer deinitFoundActions(std.testing.allocator, found);

    try std.testing.expectEqual(@as(usize, 1), found.len);
    try std.testing.expectEqualStrings("zizmor", found[0].job);
    const workflow_action = try found[0].action.allocDisplay(std.testing.allocator);
    defer std.testing.allocator.free(workflow_action);
    try std.testing.expectEqualStrings("luxass/shared-workflows/.github/workflows/reusable-ci-security.yaml", workflow_action);
    try std.testing.expectEqualStrings("luxass", found[0].action.repository.owner);
    try std.testing.expectEqualStrings("shared-workflows", found[0].action.repository.name);
    try std.testing.expectEqualStrings("v0.6.0", found[0].ref);
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
    const major_action = try found[0].action.allocDisplay(std.testing.allocator);
    defer std.testing.allocator.free(major_action);
    try std.testing.expectEqualStrings("actions/checkout", major_action);
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
    const composite_action = try found[0].action.allocDisplay(std.testing.allocator);
    defer std.testing.allocator.free(composite_action);
    try std.testing.expectEqualStrings("actions/setup-node", composite_action);
    try std.testing.expectEqualStrings("v4", found[0].ref);
    try std.testing.expectEqualStrings("v4.2.0", found[0].version_comment);
}

test "parse quoted uses span" {
    const yamlStr =
        \\jobs:
        \\  build:
        \\    steps:
        \\      - uses: "actions/setup-node@v4"
    ;

    const found = try parseWorkflowString(std.testing.allocator, ".github/workflows/ci.yml", yamlStr);
    defer deinitFoundActions(std.testing.allocator, found);

    try std.testing.expectEqual(@as(usize, 1), found.len);
    try std.testing.expectEqualStrings("v4", found[0].ref);
    try std.testing.expectEqualStrings("v4", yamlStr[found[0].ref_start..found[0].ref_end]);
}
