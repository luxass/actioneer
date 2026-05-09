const std = @import("std");
const actions = @import("../syntax/github_actions.zig");

pub fn collectReferencesFromSource(
    allocator: std.mem.Allocator,
    file_path: []const u8,
    contents: []const u8,
) ![]actions.Reference {
    return actions.collectReferences(allocator, file_path, contents);
}

pub fn parseWorkflowString(
    allocator: std.mem.Allocator,
    file_path: []const u8,
    contents: []const u8,
) ![]actions.Reference {
    return collectReferencesFromSource(allocator, file_path, contents);
}

pub fn deinitReference(allocator: std.mem.Allocator, action: actions.Reference) void {
    actions.deinitReference(allocator, action);
}

pub fn deinitReferences(allocator: std.mem.Allocator, found: []const actions.Reference) void {
    actions.deinitReferences(allocator, found);
}

pub fn deinitFoundAction(allocator: std.mem.Allocator, action: actions.Reference) void {
    deinitReference(allocator, action);
}

pub fn deinitFoundActions(allocator: std.mem.Allocator, found: []const actions.Reference) void {
    deinitReferences(allocator, found);
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
    try std.testing.expectEqual(actions.ReferenceKind.workflow_step, found[0].kind);
    try std.testing.expectEqualStrings("build", found[0].scope);
    const action_name = try found[0].name.allocDisplay(std.testing.allocator);
    defer std.testing.allocator.free(action_name);
    try std.testing.expectEqualStrings("actions/checkout", action_name);
    try std.testing.expectEqualStrings("v4", found[0].current_ref);
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
    try std.testing.expectEqual(actions.ReferenceKind.workflow_job, found[0].kind);
    try std.testing.expectEqualStrings("zizmor", found[0].scope);
    const workflow_action = try found[0].name.allocDisplay(std.testing.allocator);
    defer std.testing.allocator.free(workflow_action);
    try std.testing.expectEqualStrings("luxass/shared-workflows/.github/workflows/reusable-ci-security.yaml", workflow_action);
    try std.testing.expectEqualStrings("luxass", found[0].name.repository.owner);
    try std.testing.expectEqualStrings("shared-workflows", found[0].name.repository.name);
    try std.testing.expectEqualStrings("v0.6.0", found[0].current_ref);
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
    try std.testing.expectEqualStrings("0123456789abcdef", found[0].current_ref);
    try std.testing.expectEqualStrings("v4.1.0", found[0].version_hint);
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
    const major_action = try found[0].name.allocDisplay(std.testing.allocator);
    defer std.testing.allocator.free(major_action);
    try std.testing.expectEqualStrings("actions/checkout", major_action);
    try std.testing.expectEqualStrings("0123456789abcdef", found[0].current_ref);
    try std.testing.expectEqualStrings("v4", found[0].version_hint);
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
    try std.testing.expectEqual(actions.ReferenceKind.composite_step, found[0].kind);
    try std.testing.expectEqualStrings("composite", found[0].scope);
    const composite_action = try found[0].name.allocDisplay(std.testing.allocator);
    defer std.testing.allocator.free(composite_action);
    try std.testing.expectEqualStrings("actions/setup-node", composite_action);
    try std.testing.expectEqualStrings("v4", found[0].current_ref);
    try std.testing.expectEqualStrings("v4.2.0", found[0].version_hint);
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
    try std.testing.expectEqualStrings("v4", found[0].current_ref);
    try std.testing.expectEqualStrings("v4", yamlStr[found[0].source.ref_span.start..found[0].source.ref_span.end]);
}

test "ignore uses outside job steps and reusable workflows" {
    const yamlStr =
        \\name: ci
        \\jobs:
        \\  build:
        \\    with:
        \\      uses: not/an-action@v1
        \\    steps:
        \\      - name: setup
        \\        with:
        \\          uses: also/not-an-action@v2
        \\      - uses: actions/checkout@v4
    ;

    const found = try parseWorkflowString(std.testing.allocator, ".github/workflows/ci.yml", yamlStr);
    defer deinitFoundActions(std.testing.allocator, found);

    try std.testing.expectEqual(@as(usize, 1), found.len);
    const action_name = try found[0].name.allocDisplay(std.testing.allocator);
    defer std.testing.allocator.free(action_name);
    try std.testing.expectEqualStrings("actions/checkout", action_name);
}

test "ignore non-composite action runs uses" {
    const yamlStr =
        \\runs:
        \\  using: node20
        \\  main: dist/index.js
        \\  steps:
        \\    - uses: actions/setup-node@v4
    ;

    const found = try parseWorkflowString(std.testing.allocator, "action.yml", yamlStr);
    defer deinitFoundActions(std.testing.allocator, found);

    try std.testing.expectEqual(@as(usize, 0), found.len);
}
