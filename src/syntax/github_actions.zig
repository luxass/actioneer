const std = @import("std");
const ts = @import("tree-sitter");

const types = @import("../core/types.zig");
const yaml_tree = @import("yaml_tree.zig");

pub fn collectReferences(
    allocator: std.mem.Allocator,
    file_path: []const u8,
    contents: []const u8,
) ![]types.FoundAction {
    var found: std.ArrayList(types.FoundAction) = .empty;
    errdefer {
        for (found.items) |action| deinitFoundAction(allocator, action);
        found.deinit(allocator);
    }

    var document = try yaml_tree.parse(contents);
    defer document.deinit();

    if (isCompositeAction(file_path)) {
        try collectCompositeActions(allocator, contents, document.root_mapping, file_path, &found);
    } else {
        try collectWorkflowActions(allocator, contents, document.root_mapping, file_path, &found);
    }

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

fn collectWorkflowActions(
    allocator: std.mem.Allocator,
    contents: []const u8,
    root_mapping: ts.Node,
    file_path: []const u8,
    found: *std.ArrayList(types.FoundAction),
) anyerror!void {
    const jobs_node = yaml_tree.pairValueByKey(contents, root_mapping, "jobs") orelse return;
    const jobs_mapping = yaml_tree.mappingNode(jobs_node) orelse return;

    var index: u32 = 0;
    while (index < jobs_mapping.namedChildCount()) : (index += 1) {
        const pair = jobs_mapping.namedChild(index).?;
        const key_node = pair.childByFieldName("key") orelse continue;
        const key_range = yaml_tree.scalarRange(contents, key_node) orelse continue;
        const job_scope = yaml_tree.cleanScalar(key_range.text);
        const value_node = pair.childByFieldName("value") orelse continue;

        try collectJobActions(allocator, contents, value_node, file_path, job_scope, found);
    }
}

fn collectJobActions(
    allocator: std.mem.Allocator,
    contents: []const u8,
    job_node: ts.Node,
    file_path: []const u8,
    job_scope: []const u8,
    found: *std.ArrayList(types.FoundAction),
) anyerror!void {
    const job_mapping = yaml_tree.mappingNode(job_node) orelse return;

    if (yaml_tree.pairValueByKey(contents, job_mapping, "uses")) |value_node| {
        try appendActionReference(allocator, contents, value_node, job_scope, file_path, found);
    }

    if (yaml_tree.pairValueByKey(contents, job_mapping, "steps")) |steps_node| {
        try collectStepActions(allocator, contents, steps_node, file_path, job_scope, found);
    }
}

fn collectCompositeActions(
    allocator: std.mem.Allocator,
    contents: []const u8,
    root_mapping: ts.Node,
    file_path: []const u8,
    found: *std.ArrayList(types.FoundAction),
) anyerror!void {
    const runs_node = yaml_tree.pairValueByKey(contents, root_mapping, "runs") orelse return;
    const runs_mapping = yaml_tree.mappingNode(runs_node) orelse return;

    const using_node = yaml_tree.pairValueByKey(contents, runs_mapping, "using") orelse return;
    const using_range = yaml_tree.scalarRange(contents, using_node) orelse return;
    if (!std.mem.eql(u8, yaml_tree.cleanScalar(using_range.text), "composite")) return;

    const steps_node = yaml_tree.pairValueByKey(contents, runs_mapping, "steps") orelse return;
    try collectStepActions(allocator, contents, steps_node, file_path, "composite", found);
}

fn collectStepActions(
    allocator: std.mem.Allocator,
    contents: []const u8,
    steps_node: ts.Node,
    file_path: []const u8,
    scope: []const u8,
    found: *std.ArrayList(types.FoundAction),
) anyerror!void {
    const steps_sequence = yaml_tree.sequenceNode(steps_node) orelse return;

    var index: u32 = 0;
    while (index < steps_sequence.namedChildCount()) : (index += 1) {
        const item = steps_sequence.namedChild(index).?;
        const step_mapping = yaml_tree.mappingNode(item) orelse continue;
        const uses_node = yaml_tree.pairValueByKey(contents, step_mapping, "uses") orelse continue;
        try appendActionReference(allocator, contents, uses_node, scope, file_path, found);
    }
}

fn appendActionReference(
    allocator: std.mem.Allocator,
    contents: []const u8,
    value_node: ts.Node,
    scope: []const u8,
    file_path: []const u8,
    found: *std.ArrayList(types.FoundAction),
) !void {
    const value_range = yaml_tree.scalarRange(contents, value_node) orelse return;
    if (actionFromUsesValue(
        allocator,
        value_range.text,
        yaml_tree.extractTrailingComment(contents, value_node),
        scope,
        file_path,
        value_node.startPoint().row + 1,
        value_range.start_byte,
        value_range.end_byte,
    )) |action| {
        try found.append(allocator, action);
    } else |_| {}
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

fn isCompositeAction(path: []const u8) bool {
    return std.mem.endsWith(u8, path, "action.yml") or std.mem.endsWith(u8, path, "action.yaml");
}

test "collect workflow step and reusable workflow references" {
    const source =
        \\jobs:
        \\  build:
        \\    uses: luxass/shared-workflows/.github/workflows/ci.yml@v1
        \\    steps:
        \\      - uses: actions/checkout@v4 # v4.1.0
        \\      - uses: ./local-action
    ;

    const found = try collectReferences(std.testing.allocator, ".github/workflows/ci.yml", source);
    defer deinitFoundActions(std.testing.allocator, found);

    try std.testing.expectEqual(@as(usize, 2), found.len);

    const reusable = try found[0].action.allocDisplay(std.testing.allocator);
    defer std.testing.allocator.free(reusable);
    try std.testing.expectEqualStrings("luxass/shared-workflows/.github/workflows/ci.yml", reusable);
    try std.testing.expectEqualStrings("build", found[0].job);
    try std.testing.expectEqualStrings("v1", found[0].ref);

    const step_action = try found[1].action.allocDisplay(std.testing.allocator);
    defer std.testing.allocator.free(step_action);
    try std.testing.expectEqualStrings("actions/checkout", step_action);
    try std.testing.expectEqualStrings("v4", found[1].ref);
    try std.testing.expectEqualStrings("v4.1.0", found[1].version_comment);
}

test "collect composite action references only for composite actions" {
    const source =
        \\runs:
        \\  using: composite
        \\  steps:
        \\    - uses: actions/setup-node@v4
    ;

    const found = try collectReferences(std.testing.allocator, "action.yml", source);
    defer deinitFoundActions(std.testing.allocator, found);

    try std.testing.expectEqual(@as(usize, 1), found.len);
    try std.testing.expectEqualStrings("composite", found[0].job);
}

test "ignore non action uses sites and non composite runs" {
    const workflow_source =
        \\jobs:
        \\  build:
        \\    with:
        \\      uses: not/an-action@v1
        \\    steps:
        \\      - name: setup
        \\        with:
        \\          uses: also/not-an-action@v2
    ;
    const action_source =
        \\runs:
        \\  using: node20
        \\  steps:
        \\    - uses: actions/setup-node@v4
    ;

    const workflow_found = try collectReferences(std.testing.allocator, ".github/workflows/ci.yml", workflow_source);
    defer deinitFoundActions(std.testing.allocator, workflow_found);
    try std.testing.expectEqual(@as(usize, 0), workflow_found.len);

    const action_found = try collectReferences(std.testing.allocator, "action.yml", action_source);
    defer deinitFoundActions(std.testing.allocator, action_found);
    try std.testing.expectEqual(@as(usize, 0), action_found.len);
}
