const std = @import("std");

const types = @import("types.zig");

pub const ApplyError = error{
    UpdateTargetNotFound,
} || std.mem.Allocator.Error || std.Io.Dir.ReadFileAllocError || std.Io.Dir.WriteFileError || std.Io.Writer.Error;

pub const ApplyResult = struct {
    contents: []const u8,
    applied: usize,
};

const LineSlice = struct {
    raw: []const u8,
    text: []const u8,
    newline: []const u8,
    carriage_return: []const u8,
    next_start: usize,
};

const UsesLine = struct {
    code: []const u8,
    comment: []const u8,
    value_start: usize,
    value_end: usize,
    action_end: usize,
};

pub fn applySelected(
    allocator: std.mem.Allocator,
    io: std.Io,
    candidates: []const types.Candidate,
    selected: []const usize,
) ApplyError!usize {
    var applied: usize = 0;
    var handled_files: std.StringHashMap(void) = .init(allocator);
    defer handled_files.deinit();

    for (selected) |selected_index| {
        const file = candidates[selected_index].file;
        if (handled_files.contains(file)) continue;

        const contents = try std.Io.Dir.cwd().readFileAlloc(io, file, allocator, .limited(10 * 1024 * 1024));
        defer allocator.free(contents);

        const result = try applyToString(allocator, contents, file, candidates, selected);
        defer allocator.free(result.contents);

        if (result.applied > 0) {
            try std.Io.Dir.cwd().writeFile(io, .{
                .sub_path = file,
                .data = result.contents,
            });
            applied += result.applied;
        }

        try handled_files.put(file, {});
    }

    return applied;
}

pub fn applyToString(
    allocator: std.mem.Allocator,
    contents: []const u8,
    file: []const u8,
    candidates: []const types.Candidate,
    selected: []const usize,
) ApplyError!ApplyResult {
    var out = std.Io.Writer.Allocating.init(allocator);
    errdefer out.deinit();

    var applied: usize = 0;
    var line_number: u32 = 1;
    var start: usize = 0;

    while (start < contents.len) {
        const line = nextLine(contents, start);

        if (findCandidateForLine(candidates, selected, file, line_number)) |candidate| {
            try rewriteFileLine(&out.writer, line, candidate);
            applied += 1;
        } else {
            try out.writer.writeAll(line.raw);
        }

        start = line.next_start;
        line_number += 1;
    }

    if (selectedCountForFile(candidates, selected, file) != applied) {
        return error.UpdateTargetNotFound;
    }

    return .{
        .contents = try out.toOwnedSlice(),
        .applied = applied,
    };
}

fn findCandidateForLine(
    candidates: []const types.Candidate,
    selected: []const usize,
    file: []const u8,
    line: u32,
) ?types.Candidate {
    for (selected) |index| {
        const candidate = candidates[index];
        if (candidate.line == line and std.mem.eql(u8, candidate.file, file)) return candidate;
    }
    return null;
}

fn selectedCountForFile(candidates: []const types.Candidate, selected: []const usize, file: []const u8) usize {
    var count: usize = 0;
    for (selected) |index| {
        if (std.mem.eql(u8, candidates[index].file, file)) count += 1;
    }
    return count;
}

fn nextLine(contents: []const u8, start: usize) LineSlice {
    const newline_index = std.mem.indexOfScalarPos(u8, contents, start, '\n');
    const raw_end = newline_index orelse contents.len;
    const raw = contents[start .. raw_end + if (newline_index != null) @as(usize, 1) else 0];
    const newline = if (newline_index != null) contents[raw_end .. raw_end + 1] else "";
    const text_end = if (raw_end > start and contents[raw_end - 1] == '\r') raw_end - 1 else raw_end;

    return .{
        .raw = raw,
        .text = contents[start..text_end],
        .newline = newline,
        .carriage_return = if (text_end != raw_end) "\r" else "",
        .next_start = raw_end + if (newline_index != null) @as(usize, 1) else 0,
    };
}

fn rewriteFileLine(writer: *std.Io.Writer, line: LineSlice, candidate: types.Candidate) ApplyError!void {
    try rewriteLineText(writer, line.text, candidate);
    try writer.writeAll(line.carriage_return);
    try writer.writeAll(line.newline);
}

fn rewriteLineText(writer: *std.Io.Writer, line: []const u8, candidate: types.Candidate) ApplyError!void {
    const parsed = try parseUsesLine(line);

    const value = parsed.code[parsed.value_start..parsed.value_end];
    if (!std.mem.eql(u8, value[0..parsed.action_end], candidate.action)) return error.UpdateTargetNotFound;

    const ref_start = parsed.value_start + parsed.action_end + 1;
    try writeUpdatedUses(writer, parsed, ref_start, candidate);
}

fn parseUsesLine(line: []const u8) ApplyError!UsesLine {
    const comment_start = findCommentStart(line);
    const code = line[0 .. comment_start orelse line.len];
    const comment = if (comment_start) |index| line[index..] else "";

    const value_start = try findUsesValueStart(code);
    const value_end = findUsesValueEnd(code, value_start) orelse return error.UpdateTargetNotFound;
    const value = code[value_start..value_end];
    const action_end = std.mem.lastIndexOfScalar(u8, value, '@') orelse return error.UpdateTargetNotFound;

    return .{
        .code = code,
        .comment = comment,
        .value_start = value_start,
        .value_end = value_end,
        .action_end = action_end,
    };
}

fn findUsesValueStart(code: []const u8) ApplyError!usize {
    const uses_index = std.mem.indexOf(u8, code, "uses:") orelse return error.UpdateTargetNotFound;
    var value_start = uses_index + "uses:".len;
    while (value_start < code.len and (code[value_start] == ' ' or code[value_start] == '\t')) : (value_start += 1) {}

    if (value_start < code.len and (code[value_start] == '"' or code[value_start] == '\'')) {
        value_start += 1;
    }

    return value_start;
}

fn findUsesValueEnd(code: []const u8, value_start: usize) ?usize {
    if (value_start == 0) return null;

    const opening_quote_index = value_start - 1;
    if (opening_quote_index < code.len and (code[opening_quote_index] == '"' or code[opening_quote_index] == '\'')) {
        return std.mem.indexOfScalarPos(u8, code, value_start, code[opening_quote_index]);
    }

    return std.mem.trimEnd(u8, code[value_start..], " \t").len + value_start;
}

fn writeUpdatedUses(
    writer: *std.Io.Writer,
    parsed: UsesLine,
    ref_start: usize,
    candidate: types.Candidate,
) ApplyError!void {
    try writer.writeAll(parsed.code[0..ref_start]);
    try writer.writeAll(candidate.next);
    try writer.writeAll(parsed.code[parsed.value_end..]);
    try writeComment(writer, parsed.comment, candidate);
}

fn writeComment(writer: *std.Io.Writer, existing_comment: []const u8, candidate: types.Candidate) ApplyError!void {
    if (shouldWriteVersionComment(candidate)) {
        try writer.writeAll(" # ");
        try writer.writeAll(displayTarget(candidate));
    } else {
        try writer.writeAll(existing_comment);
    }
}

fn shouldWriteVersionComment(candidate: types.Candidate) bool {
    const target = displayTarget(candidate);
    return target.len > 0 and
        (!std.mem.eql(u8, candidate.next, target) or candidate.version_comment.len > 0 or candidate.sha_mismatch);
}

fn displayTarget(candidate: types.Candidate) []const u8 {
    return if (candidate.next_label.len > 0) candidate.next_label else candidate.next;
}

fn findCommentStart(line: []const u8) ?usize {
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
        if (char == '#') return index;
    }
    return null;
}

test "apply sha update and version comment" {
    const input =
        \\jobs:
        \\  build:
        \\    steps:
        \\      - uses: actions/checkout@oldsha # v4.1.0
        \\      - uses: actions/setup-node@v3
        \\
    ;
    const candidates = [_]types.Candidate{
        .{
            .action = "actions/checkout",
            .job = "build",
            .current = "oldsha",
            .version_comment = "v4.1.0",
            .next = "newsha",
            .next_label = "v4.2.0",
            .file = ".github/workflows/ci.yml",
            .line = 4,
        },
    };

    const result = try applyToString(std.testing.allocator, input, ".github/workflows/ci.yml", &candidates, &.{0});
    defer std.testing.allocator.free(result.contents);

    try std.testing.expectEqual(@as(usize, 1), result.applied);
    try std.testing.expectEqualStrings(
        \\jobs:
        \\  build:
        \\    steps:
        \\      - uses: actions/checkout@newsha # v4.2.0
        \\      - uses: actions/setup-node@v3
        \\
    , result.contents);
}

test "parse uses line with comment" {
    const parsed = try parseUsesLine("      - uses: actions/checkout@oldsha # v4.1.0");

    try std.testing.expectEqualStrings("actions/checkout@oldsha", parsed.code[parsed.value_start..parsed.value_end]);
    try std.testing.expectEqual(@as(usize, "actions/checkout".len), parsed.action_end);
    try std.testing.expectEqualStrings("# v4.1.0", parsed.comment);
}

test "parse quoted uses line" {
    const parsed = try parseUsesLine("  uses: 'actions/setup-node@v3'");

    try std.testing.expectEqualStrings("actions/setup-node@v3", parsed.code[parsed.value_start..parsed.value_end]);
    try std.testing.expectEqual(@as(usize, "actions/setup-node".len), parsed.action_end);
    try std.testing.expectEqualStrings("", parsed.comment);
}

test "apply quoted version update without adding comment" {
    const input =
        \\steps:
        \\  - uses: "actions/setup-node@v3"
        \\
    ;
    const candidates = [_]types.Candidate{
        .{
            .action = "actions/setup-node",
            .job = "build",
            .current = "v3",
            .next = "v4",
            .next_label = "v4",
            .file = "ci.yml",
            .line = 2,
        },
    };

    const result = try applyToString(std.testing.allocator, input, "ci.yml", &candidates, &.{0});
    defer std.testing.allocator.free(result.contents);

    try std.testing.expectEqualStrings(
        \\steps:
        \\  - uses: "actions/setup-node@v4"
        \\
    , result.contents);
}

test "apply reusable workflow update" {
    const input =
        \\jobs:
        \\  zizmor:
        \\    uses: luxass/shared-workflows/.github/workflows/reusable-ci-security.yaml@v0.6.0
        \\
    ;
    const candidates = [_]types.Candidate{
        .{
            .action = "luxass/shared-workflows/.github/workflows/reusable-ci-security.yaml",
            .job = "zizmor",
            .current = "v0.6.0",
            .next = "v0.7.0",
            .next_label = "v0.7.0",
            .file = "ci-security.yml",
            .line = 3,
        },
    };

    const result = try applyToString(std.testing.allocator, input, "ci-security.yml", &candidates, &.{0});
    defer std.testing.allocator.free(result.contents);

    try std.testing.expectEqualStrings(
        \\jobs:
        \\  zizmor:
        \\    uses: luxass/shared-workflows/.github/workflows/reusable-ci-security.yaml@v0.7.0
        \\
    , result.contents);
}
