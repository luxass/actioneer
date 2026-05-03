const std = @import("std");

pub const Document = struct {
    allocator: std.mem.Allocator,
    root: *Node,

    pub fn deinit(self: Document) void {
        self.root.deinit(self.allocator);
        self.allocator.destroy(self.root);
    }
};

pub const Node = struct {
    value: Value,
    line: u32,
    comment: []const u8 = "",

    pub fn deinit(self: *Node, allocator: std.mem.Allocator) void {
        switch (self.value) {
            .scalar => |scalar_value| allocator.free(scalar_value),
            .map => |pairs| {
                for (pairs) |pair| {
                    allocator.free(pair.key);
                    pair.value.deinit(allocator);
                    allocator.destroy(pair.value);
                }
                allocator.free(pairs);
            },
            .list => |items| {
                for (items) |item| {
                    item.deinit(allocator);
                    allocator.destroy(item);
                }
                allocator.free(items);
            },
            .null => {},
        }
        if (self.comment.len > 0) allocator.free(self.comment);
    }

    pub fn get(self: *const Node, key: []const u8) ?*Node {
        if (self.value != .map) return null;
        for (self.value.map) |pair| {
            if (std.mem.eql(u8, pair.key, key)) return pair.value;
        }
        return null;
    }

    pub fn scalar(self: *const Node) ?[]const u8 {
        return switch (self.value) {
            .scalar => |value| value,
            else => null,
        };
    }
};

pub const Pair = struct {
    key: []const u8,
    value: *Node,
};

pub const Value = union(enum) {
    scalar: []const u8,
    map: []Pair,
    list: []*Node,
    null,
};

const Line = struct {
    indent: usize,
    text: []const u8,
    comment: []const u8 = "",
    number: u32,
};

pub fn parseYaml(allocator: std.mem.Allocator, input: []const u8) !Document {
    var lines = try collectLines(allocator, input);
    defer lines.deinit(allocator);

    var index: usize = 0;
    const root = try parseBlock(allocator, lines.items, &index, if (lines.items.len > 0) lines.items[0].indent else 0);

    return .{
        .allocator = allocator,
        .root = root,
    };
}

fn collectLines(allocator: std.mem.Allocator, input: []const u8) !std.ArrayList(Line) {
    var lines: std.ArrayList(Line) = .empty;
    errdefer lines.deinit(allocator);

    var iter = std.mem.splitScalar(u8, input, '\n');
    var number: u32 = 0;
    while (iter.next()) |raw_line| {
        number += 1;
        const line = std.mem.trimEnd(u8, raw_line, "\r");
        const trimmed = std.mem.trim(u8, line, " \t");
        if (trimmed.len == 0 or trimmed[0] == '#') continue;

        const split = splitValueAndComment(line);
        const text = std.mem.trim(u8, split.value, " \t");
        if (text.len == 0) continue;

        try lines.append(allocator, .{
            .indent = leadingSpaces(line),
            .text = text,
            .comment = split.comment orelse "",
            .number = number,
        });
    }

    return lines;
}

fn parseBlock(allocator: std.mem.Allocator, lines: []const Line, index: *usize, indent: usize) anyerror!*Node {
    if (index.* >= lines.len or lines[index.*].indent < indent) {
        return createNode(allocator, .null, 0, "");
    }

    if (std.mem.startsWith(u8, lines[index.*].text, "- ")) {
        return parseList(allocator, lines, index, indent);
    }
    return parseMap(allocator, lines, index, indent);
}

fn parseMap(allocator: std.mem.Allocator, lines: []const Line, index: *usize, indent: usize) anyerror!*Node {
    var pairs: std.ArrayList(Pair) = .empty;
    errdefer deinitPairs(allocator, pairs.items);

    const start_line = if (index.* < lines.len) lines[index.*].number else 0;

    while (index.* < lines.len) {
        const line = lines[index.*];
        if (line.indent < indent) break;
        if (line.indent > indent) break;
        if (std.mem.startsWith(u8, line.text, "- ")) break;

        const colon = std.mem.indexOfScalar(u8, line.text, ':') orelse break;
        const key = std.mem.trim(u8, line.text[0..colon], " \t");
        const rest = std.mem.trim(u8, line.text[colon + 1 ..], " \t");

        index.* += 1;

        const value = if (isBlockScalarIndicator(rest))
            try createBlockScalarNode(allocator, lines, index, line.indent, line.number, line.comment)
        else if (rest.len > 0)
            try createScalarNode(allocator, rest, line.number, line.comment)
        else if (index.* < lines.len and lines[index.*].indent > line.indent)
            try parseBlock(allocator, lines, index, lines[index.*].indent)
        else
            try createNode(allocator, .null, line.number, line.comment);

        try pairs.append(allocator, .{
            .key = try allocator.dupe(u8, cleanScalar(key)),
            .value = value,
        });
    }

    return createNode(allocator, .{ .map = try pairs.toOwnedSlice(allocator) }, start_line, "");
}

fn parseList(allocator: std.mem.Allocator, lines: []const Line, index: *usize, indent: usize) anyerror!*Node {
    var items: std.ArrayList(*Node) = .empty;
    errdefer deinitItems(allocator, items.items);

    const start_line = if (index.* < lines.len) lines[index.*].number else 0;

    while (index.* < lines.len) {
        const line = lines[index.*];
        if (line.indent < indent) break;
        if (line.indent > indent) break;
        if (!std.mem.startsWith(u8, line.text, "- ")) break;

        const rest = std.mem.trim(u8, line.text[2..], " \t");
        index.* += 1;

        const item = if (isBlockScalarIndicator(rest))
            try createBlockScalarNode(allocator, lines, index, line.indent, line.number, line.comment)
        else if (rest.len == 0)
            try parseBlock(allocator, lines, index, if (index.* < lines.len) lines[index.*].indent else indent + 2)
        else if (std.mem.indexOfScalar(u8, rest, ':') != null)
            try parseInlineMapItem(allocator, lines, index, indent, line, rest)
        else
            try createScalarNode(allocator, rest, line.number, line.comment);

        try items.append(allocator, item);
    }

    return createNode(allocator, .{ .list = try items.toOwnedSlice(allocator) }, start_line, "");
}

fn parseInlineMapItem(
    allocator: std.mem.Allocator,
    lines: []const Line,
    index: *usize,
    list_indent: usize,
    line: Line,
    rest: []const u8,
) !*Node {
    var pairs: std.ArrayList(Pair) = .empty;
    errdefer deinitPairs(allocator, pairs.items);

    try appendInlinePair(allocator, &pairs, lines, index, line.indent, line, rest);

    while (index.* < lines.len and lines[index.*].indent > list_indent) {
        const child_line = lines[index.*];
        if (std.mem.startsWith(u8, child_line.text, "- ")) break;

        const colon = std.mem.indexOfScalar(u8, child_line.text, ':') orelse break;
        const key = std.mem.trim(u8, child_line.text[0..colon], " \t");
        const value_text = std.mem.trim(u8, child_line.text[colon + 1 ..], " \t");
        index.* += 1;

        const value = if (isBlockScalarIndicator(value_text))
            try createBlockScalarNode(allocator, lines, index, child_line.indent, child_line.number, child_line.comment)
        else if (value_text.len > 0)
            try createScalarNode(allocator, value_text, child_line.number, child_line.comment)
        else if (index.* < lines.len and lines[index.*].indent > child_line.indent)
            try parseBlock(allocator, lines, index, lines[index.*].indent)
        else
            try createNode(allocator, .null, child_line.number, child_line.comment);

        try pairs.append(allocator, .{
            .key = try allocator.dupe(u8, cleanScalar(key)),
            .value = value,
        });
    }

    return createNode(allocator, .{ .map = try pairs.toOwnedSlice(allocator) }, line.number, "");
}

fn appendInlinePair(
    allocator: std.mem.Allocator,
    pairs: *std.ArrayList(Pair),
    lines: []const Line,
    index: *usize,
    parent_indent: usize,
    line: Line,
    text: []const u8,
) !void {
    const colon = std.mem.indexOfScalar(u8, text, ':') orelse return error.InvalidYaml;
    const key = std.mem.trim(u8, text[0..colon], " \t");
    const rest = std.mem.trim(u8, text[colon + 1 ..], " \t");

    const value = if (isBlockScalarIndicator(rest))
        try createBlockScalarNode(allocator, lines, index, parent_indent, line.number, line.comment)
    else if (rest.len > 0)
        try createScalarNode(allocator, rest, line.number, line.comment)
    else
        try createNode(allocator, .null, line.number, line.comment);

    try pairs.append(allocator, .{
        .key = try allocator.dupe(u8, cleanScalar(key)),
        .value = value,
    });
}

fn createScalarNode(allocator: std.mem.Allocator, value: []const u8, line: u32, comment: []const u8) !*Node {
    return createNode(allocator, .{ .scalar = try allocator.dupe(u8, cleanScalar(value)) }, line, comment);
}

fn createBlockScalarNode(
    allocator: std.mem.Allocator,
    lines: []const Line,
    index: *usize,
    parent_indent: usize,
    line: u32,
    comment: []const u8,
) !*Node {
    var value: std.ArrayList(u8) = .empty;
    errdefer value.deinit(allocator);

    while (index.* < lines.len and lines[index.*].indent > parent_indent) : (index.* += 1) {
        if (value.items.len > 0) try value.append(allocator, '\n');
        try value.appendSlice(allocator, lines[index.*].text);
    }

    return createNode(allocator, .{ .scalar = try value.toOwnedSlice(allocator) }, line, comment);
}

fn isBlockScalarIndicator(value: []const u8) bool {
    return std.mem.eql(u8, value, "|") or
        std.mem.eql(u8, value, ">") or
        std.mem.startsWith(u8, value, "|+") or
        std.mem.startsWith(u8, value, "|-") or
        std.mem.startsWith(u8, value, ">+") or
        std.mem.startsWith(u8, value, ">-");
}

fn createNode(allocator: std.mem.Allocator, value: Value, line: u32, comment: []const u8) !*Node {
    const node = try allocator.create(Node);
    node.* = .{
        .value = value,
        .line = line,
        .comment = if (comment.len > 0) try allocator.dupe(u8, comment) else "",
    };
    return node;
}

fn deinitPairs(allocator: std.mem.Allocator, pairs: []Pair) void {
    for (pairs) |pair| {
        allocator.free(pair.key);
        pair.value.deinit(allocator);
        allocator.destroy(pair.value);
    }
    allocator.free(pairs);
}

fn deinitItems(allocator: std.mem.Allocator, items: []*Node) void {
    for (items) |item| {
        item.deinit(allocator);
        allocator.destroy(item);
    }
    allocator.free(items);
}

const ValueAndComment = struct {
    value: []const u8,
    comment: ?[]const u8,
};

fn splitValueAndComment(line: []const u8) ValueAndComment {
    var quote: ?u8 = null;
    for (line, 0..) |char, index| {
        if (quote) |active| {
            if (char == active) quote = null;
            continue;
        }
        if (char == '"' or char == '\'') {
            quote = char;
            continue;
        }
        if (char == '#') {
            return .{
                .value = std.mem.trimEnd(u8, line[0..index], " \t"),
                .comment = std.mem.trim(u8, line[index + 1 ..], " \t"),
            };
        }
    }
    return .{ .value = line, .comment = null };
}

fn cleanScalar(value: []const u8) []const u8 {
    var result = std.mem.trim(u8, value, " \t");
    if (result.len >= 2) {
        const first = result[0];
        const last = result[result.len - 1];
        if ((first == '"' and last == '"') or (first == '\'' and last == '\'')) {
            return result[1 .. result.len - 1];
        }
    }
    return result;
}

fn leadingSpaces(line: []const u8) usize {
    var count: usize = 0;
    while (count < line.len and line[count] == ' ') : (count += 1) {}
    return count;
}

test "parse yaml into map and list objects" {
    const yaml =
        \\name: ci
        \\jobs:
        \\  build:
        \\    steps:
        \\      - uses: actions/checkout@v4 # v4
        \\      - name: setup
        \\        uses: "pnpm/action-setup@v4.1.0"
    ;

    const document = try parseYaml(std.testing.allocator, yaml);
    defer document.deinit();

    const jobs = document.root.get("jobs").?;
    const build = jobs.get("build").?;
    const steps = build.get("steps").?;
    try std.testing.expect(steps.value == .list);
    try std.testing.expectEqual(@as(usize, 2), steps.value.list.len);

    const first = steps.value.list[0];
    try std.testing.expectEqualStrings("actions/checkout@v4", first.get("uses").?.scalar().?);
    try std.testing.expectEqualStrings("v4", first.get("uses").?.comment);

    const second = steps.value.list[1];
    try std.testing.expectEqualStrings("setup", second.get("name").?.scalar().?);
    try std.testing.expectEqualStrings("pnpm/action-setup@v4.1.0", second.get("uses").?.scalar().?);
}

test "parse yaml list of scalar values" {
    const yaml =
        \\on:
        \\  - push
        \\  - pull_request
    ;

    const document = try parseYaml(std.testing.allocator, yaml);
    defer document.deinit();

    const on = document.root.get("on").?;
    try std.testing.expect(on.value == .list);
    try std.testing.expectEqualStrings("push", on.value.list[0].scalar().?);
    try std.testing.expectEqualStrings("pull_request", on.value.list[1].scalar().?);
}

test "parse block scalar without swallowing following list items" {
    const yaml =
        \\steps:
        \\  - name: script
        \\    run: |
        \\      echo one
        \\      echo two
        \\  - name: checkout
        \\    uses: actions/checkout@v4
    ;

    const document = try parseYaml(std.testing.allocator, yaml);
    defer document.deinit();

    const steps = document.root.get("steps").?;
    try std.testing.expect(steps.value == .list);
    try std.testing.expectEqual(@as(usize, 2), steps.value.list.len);
    try std.testing.expectEqualStrings("echo one\necho two", steps.value.list[0].get("run").?.scalar().?);
    try std.testing.expectEqualStrings("actions/checkout@v4", steps.value.list[1].get("uses").?.scalar().?);
}
