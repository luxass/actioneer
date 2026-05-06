const std = @import("std");
const ts = @import("tree-sitter");

extern fn tree_sitter_yaml() callconv(.c) *const ts.Language;

pub const ScalarRange = struct {
    text: []const u8,
    start_byte: u32,
    end_byte: u32,
};

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

pub const Document = struct {
    tree: *ts.Tree,
    root_mapping: ts.Node,

    pub fn deinit(self: Document) void {
        self.tree.destroy();
    }
};

pub fn parse(contents: []const u8) !Document {
    const parser = ts.Parser.create();
    defer parser.destroy();
    try parser.setLanguage(tree_sitter_yaml());

    const tree = parser.parseString(contents, null) orelse return error.InvalidYaml;
    const root_mapping = mappingNode(tree.rootNode()) orelse return .{
        .tree = tree,
        .root_mapping = tree.rootNode(),
    };
    return .{
        .tree = tree,
        .root_mapping = root_mapping,
    };
}

pub fn pairValueByKey(contents: []const u8, mapping: ts.Node, key: []const u8) ?ts.Node {
    const actual_mapping = mappingNode(mapping) orelse return null;

    var index: u32 = 0;
    while (index < actual_mapping.namedChildCount()) : (index += 1) {
        const pair = actual_mapping.namedChild(index).?;
        if (!PairKinds.has(pair.kind())) continue;

        const key_node = pair.childByFieldName("key") orelse continue;
        const key_range = scalarRange(contents, key_node) orelse continue;
        if (std.mem.eql(u8, key_range.text, key)) return pair.childByFieldName("value");
    }

    return null;
}

pub fn mappingNode(node: ts.Node) ?ts.Node {
    const kind = node.kind();
    if (MappingKinds.has(kind)) return node;
    if (WrapperKinds.has(kind) or std.mem.eql(u8, kind, "block_sequence_item")) {
        var index: u32 = 0;
        while (index < node.namedChildCount()) : (index += 1) {
            if (mappingNode(node.namedChild(index).?)) |mapping| return mapping;
        }
    }
    return null;
}

pub fn sequenceNode(node: ts.Node) ?ts.Node {
    const kind = node.kind();
    if (std.mem.eql(u8, kind, "block_sequence") or std.mem.eql(u8, kind, "flow_sequence")) return node;
    if (WrapperKinds.has(kind)) {
        var index: u32 = 0;
        while (index < node.namedChildCount()) : (index += 1) {
            if (sequenceNode(node.namedChild(index).?)) |sequence| return sequence;
        }
    }
    return null;
}

pub fn scalarRange(contents: []const u8, node: ts.Node) ?ScalarRange {
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

pub fn extractTrailingComment(contents: []const u8, value_node: ts.Node) []const u8 {
    const end = value_node.endByte();
    const line_end = std.mem.indexOfScalarPos(u8, contents, end, '\n') orelse contents.len;
    const tail = contents[end..line_end];
    const comment_start = std.mem.indexOfScalar(u8, tail, '#') orelse return "";
    return std.mem.trim(u8, tail[comment_start + 1 ..], " \t");
}

pub fn cleanScalar(value: []const u8) []const u8 {
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

test "parse finds root mapping and nested job mapping" {
    const source =
        \\name: ci
        \\jobs:
        \\  build:
        \\    steps:
        \\      - uses: actions/checkout@v4
    ;

    var document = try parse(source);
    defer document.deinit();

    const jobs_node = pairValueByKey(source, document.root_mapping, "jobs") orelse return error.TestUnexpectedResult;
    const jobs_mapping = mappingNode(jobs_node) orelse return error.TestUnexpectedResult;
    const build_node = pairValueByKey(source, jobs_mapping, "build") orelse return error.TestUnexpectedResult;
    try std.testing.expect(mappingNode(build_node) != null);
}

test "sequence node finds wrapped steps sequence" {
    const source =
        \\steps:
        \\  - uses: actions/checkout@v4
        \\  - uses: actions/setup-node@v4
    ;

    var document = try parse(source);
    defer document.deinit();

    const steps_node = pairValueByKey(source, document.root_mapping, "steps") orelse return error.TestUnexpectedResult;
    const steps_sequence = sequenceNode(steps_node) orelse return error.TestUnexpectedResult;
    try std.testing.expectEqual(@as(u32, 2), steps_sequence.namedChildCount());
}

test "scalar range removes quotes and preserves ref span" {
    const source =
        \\uses: "actions/setup-node@v4"
    ;

    var document = try parse(source);
    defer document.deinit();

    const uses_node = pairValueByKey(source, document.root_mapping, "uses") orelse return error.TestUnexpectedResult;
    const value = scalarRange(source, uses_node) orelse return error.TestUnexpectedResult;

    try std.testing.expectEqualStrings("actions/setup-node@v4", value.text);
    try std.testing.expectEqualStrings("actions/setup-node@v4", source[value.start_byte..value.end_byte]);
}

test "extract trailing comment ignores quoted hash and returns comment text" {
    const source =
        \\uses: "actions/setup-node@v4#literal" # v4.2.0
    ;

    var document = try parse(source);
    defer document.deinit();

    const uses_node = pairValueByKey(source, document.root_mapping, "uses") orelse return error.TestUnexpectedResult;
    try std.testing.expectEqualStrings("v4.2.0", extractTrailingComment(source, uses_node));
}
