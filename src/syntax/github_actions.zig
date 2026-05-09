const std = @import("std");
const ts = @import("tree-sitter");

const yaml_tree = @import("yaml_tree.zig");

pub const ReferenceKind = enum {
    workflow_job,
    workflow_step,
    composite_step,
};

pub const ByteSpan = struct {
    start: u32,
    end: u32,
};

pub const SourceLocation = struct {
    file: []const u8,
    line: u32,
    ref_span: ByteSpan,
};

pub const Repository = struct {
    owner: []const u8,
    name: []const u8,

    pub fn allocDisplay(self: Repository, allocator: anytype) ![]const u8 {
        return std.fmt.allocPrint(allocator, "{s}/{s}", .{ self.owner, self.name });
    }
};

pub const ActionName = struct {
    repository: Repository,
    path: []const u8 = "",

    pub fn displayLen(self: ActionName) usize {
        return self.repository.owner.len + 1 + self.repository.name.len + self.path.len;
    }

    pub fn allocDisplay(self: ActionName, allocator: anytype) ![]const u8 {
        return std.fmt.allocPrint(allocator, "{s}/{s}{s}", .{ self.repository.owner, self.repository.name, self.path });
    }

    pub fn eqlDisplay(self: ActionName, value: []const u8) bool {
        if (value.len != self.displayLen()) return false;
        if (!std.mem.startsWith(u8, value, self.repository.owner)) return false;
        if (value[self.repository.owner.len] != '/') return false;

        const name_start = self.repository.owner.len + 1;
        if (!std.mem.startsWith(u8, value[name_start..], self.repository.name)) return false;
        return std.mem.eql(u8, value[name_start + self.repository.name.len ..], self.path);
    }
};

pub const Reference = struct {
    kind: ReferenceKind,
    name: ActionName,
    current_ref: []const u8,
    version_hint: []const u8 = "",
    scope: []const u8,
    source: SourceLocation,
};

pub fn collectReferences(
    allocator: std.mem.Allocator,
    file_path: []const u8,
    contents: []const u8,
) ![]Reference {
    var found: std.ArrayList(Reference) = .empty;
    errdefer {
        for (found.items) |action| deinitReference(allocator, action);
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

pub fn deinitReference(allocator: std.mem.Allocator, action: Reference) void {
    allocator.free(action.name.repository.owner);
    allocator.free(action.name.repository.name);
    if (action.name.path.len > 0) allocator.free(action.name.path);
    allocator.free(action.current_ref);
    if (action.version_hint.len > 0) allocator.free(action.version_hint);
    allocator.free(action.scope);
    allocator.free(action.source.file);
}

pub fn deinitReferences(allocator: std.mem.Allocator, found: []const Reference) void {
    for (found) |action| deinitReference(allocator, action);
    allocator.free(found);
}

fn collectWorkflowActions(
    allocator: std.mem.Allocator,
    contents: []const u8,
    root_mapping: ts.Node,
    file_path: []const u8,
    found: *std.ArrayList(Reference),
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
    found: *std.ArrayList(Reference),
) anyerror!void {
    const job_mapping = yaml_tree.mappingNode(job_node) orelse return;

    if (yaml_tree.pairValueByKey(contents, job_mapping, "uses")) |value_node| {
        try appendActionReference(allocator, contents, value_node, job_scope, .workflow_job, file_path, found);
    }

    if (yaml_tree.pairValueByKey(contents, job_mapping, "steps")) |steps_node| {
        try collectStepActions(allocator, contents, steps_node, file_path, job_scope, .workflow_step, found);
    }
}

fn collectCompositeActions(
    allocator: std.mem.Allocator,
    contents: []const u8,
    root_mapping: ts.Node,
    file_path: []const u8,
    found: *std.ArrayList(Reference),
) anyerror!void {
    const runs_node = yaml_tree.pairValueByKey(contents, root_mapping, "runs") orelse return;
    const runs_mapping = yaml_tree.mappingNode(runs_node) orelse return;

    const using_node = yaml_tree.pairValueByKey(contents, runs_mapping, "using") orelse return;
    const using_range = yaml_tree.scalarRange(contents, using_node) orelse return;
    if (!std.mem.eql(u8, yaml_tree.cleanScalar(using_range.text), "composite")) return;

    const steps_node = yaml_tree.pairValueByKey(contents, runs_mapping, "steps") orelse return;
    try collectStepActions(allocator, contents, steps_node, file_path, "composite", .composite_step, found);
}

fn collectStepActions(
    allocator: std.mem.Allocator,
    contents: []const u8,
    steps_node: ts.Node,
    file_path: []const u8,
    scope: []const u8,
    kind: ReferenceKind,
    found: *std.ArrayList(Reference),
) anyerror!void {
    const steps_sequence = yaml_tree.sequenceNode(steps_node) orelse return;

    var index: u32 = 0;
    while (index < steps_sequence.namedChildCount()) : (index += 1) {
        const item = steps_sequence.namedChild(index).?;
        const step_mapping = yaml_tree.mappingNode(item) orelse continue;
        const uses_node = yaml_tree.pairValueByKey(contents, step_mapping, "uses") orelse continue;
        try appendActionReference(allocator, contents, uses_node, scope, kind, file_path, found);
    }
}

fn appendActionReference(
    allocator: std.mem.Allocator,
    contents: []const u8,
    value_node: ts.Node,
    scope: []const u8,
    kind: ReferenceKind,
    file_path: []const u8,
    found: *std.ArrayList(Reference),
) !void {
    const value_range = yaml_tree.scalarRange(contents, value_node) orelse return;
    if (actionFromUsesValue(
        allocator,
        value_range.text,
        yaml_tree.extractTrailingComment(contents, value_node),
        scope,
        kind,
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
    kind: ReferenceKind,
    file_path: []const u8,
    line: u32,
    value_start: u32,
    value_end: u32,
) !Reference {
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
        .kind = kind,
        .name = .{
            .repository = .{
                .owner = try allocator.dupe(u8, parsed.owner),
                .name = try allocator.dupe(u8, parsed.name),
            },
            .path = if (parsed.path.len > 0) try allocator.dupe(u8, parsed.path) else "",
        },
        .current_ref = try allocator.dupe(u8, parsed.ref),
        .version_hint = if (version_comment) |version| try allocator.dupe(u8, version) else "",
        .scope = try allocator.dupe(u8, scope),
        .source = .{
            .file = try allocator.dupe(u8, file_path),
            .line = line,
            .ref_span = .{
                .start = ref_start,
                .end = value_end,
            },
        },
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
    defer deinitReferences(std.testing.allocator, found);

    try std.testing.expectEqual(@as(usize, 2), found.len);
    try std.testing.expectEqual(ReferenceKind.workflow_job, found[0].kind);
    try std.testing.expectEqual(ReferenceKind.workflow_step, found[1].kind);

    const reusable = try found[0].name.allocDisplay(std.testing.allocator);
    defer std.testing.allocator.free(reusable);
    try std.testing.expectEqualStrings("luxass/shared-workflows/.github/workflows/ci.yml", reusable);
    try std.testing.expectEqualStrings("build", found[0].scope);
    try std.testing.expectEqualStrings("v1", found[0].current_ref);

    const step_action = try found[1].name.allocDisplay(std.testing.allocator);
    defer std.testing.allocator.free(step_action);
    try std.testing.expectEqualStrings("actions/checkout", step_action);
    try std.testing.expectEqualStrings("v4", found[1].current_ref);
    try std.testing.expectEqualStrings("v4.1.0", found[1].version_hint);
}

test "collect composite action references only for composite actions" {
    const source =
        \\runs:
        \\  using: composite
        \\  steps:
        \\    - uses: actions/setup-node@v4
    ;

    const found = try collectReferences(std.testing.allocator, "action.yml", source);
    defer deinitReferences(std.testing.allocator, found);

    try std.testing.expectEqual(@as(usize, 1), found.len);
    try std.testing.expectEqual(ReferenceKind.composite_step, found[0].kind);
    try std.testing.expectEqualStrings("composite", found[0].scope);
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
    defer deinitReferences(std.testing.allocator, workflow_found);
    try std.testing.expectEqual(@as(usize, 0), workflow_found.len);

    const action_found = try collectReferences(std.testing.allocator, "action.yml", action_source);
    defer deinitReferences(std.testing.allocator, action_found);
    try std.testing.expectEqual(@as(usize, 0), action_found.len);
}
