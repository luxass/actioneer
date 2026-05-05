const std = @import("std");

const parse = @import("parse.zig");
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
    const file_candidates = try selectedCandidatesForFile(allocator, candidates, selected, file);
    defer allocator.free(file_candidates);

    var out = std.Io.Writer.Allocating.init(allocator);
    errdefer out.deinit();

    var cursor: usize = 0;

    for (file_candidates) |candidate| {
        if (candidate.ref_start > candidate.ref_end or candidate.ref_end > contents.len or candidate.ref_start < cursor) {
            return error.UpdateTargetNotFound;
        }
        if (!std.mem.eql(u8, contents[candidate.ref_start..candidate.ref_end], candidate.current)) {
            return error.UpdateTargetNotFound;
        }

        const line_start = lineStart(contents, candidate.ref_start);
        const line = nextLine(contents, line_start);
        if (line_start < cursor) return error.UpdateTargetNotFound;

        try out.writer.writeAll(contents[cursor..line_start]);
        try rewriteFileLine(&out.writer, line, line_start, candidate);
        cursor = line.next_start;
    }

    try out.writer.writeAll(contents[cursor..]);

    return .{
        .contents = try out.toOwnedSlice(),
        .applied = file_candidates.len,
    };
}

fn selectedCandidatesForFile(
    allocator: std.mem.Allocator,
    candidates: []const types.Candidate,
    selected: []const usize,
    file: []const u8,
) ![]types.Candidate {
    var filtered: std.ArrayList(types.Candidate) = .empty;
    defer filtered.deinit(allocator);

    for (selected) |index| {
        const candidate = candidates[index];
        if (!std.mem.eql(u8, candidate.file, file)) continue;
        try filtered.append(allocator, candidate);
    }

    std.sort.insertion(types.Candidate, filtered.items, {}, lessThanCandidate);
    return filtered.toOwnedSlice(allocator);
}

fn lessThanCandidate(_: void, lhs: types.Candidate, rhs: types.Candidate) bool {
    return lhs.ref_start < rhs.ref_start;
}

fn lineStart(contents: []const u8, offset: usize) usize {
    return if (std.mem.lastIndexOfScalar(u8, contents[0..offset], '\n')) |index| index + 1 else 0;
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

fn rewriteFileLine(writer: *std.Io.Writer, line: LineSlice, line_start: usize, candidate: types.Candidate) ApplyError!void {
    try rewriteLineText(writer, line.text, line_start, candidate);
    try writer.writeAll(line.carriage_return);
    try writer.writeAll(line.newline);
}

fn rewriteLineText(writer: *std.Io.Writer, line: []const u8, line_start: usize, candidate: types.Candidate) ApplyError!void {
    const rel_ref_start = candidate.ref_start - line_start;
    const rel_ref_end = candidate.ref_end - line_start;
    if (rel_ref_end > line.len) return error.UpdateTargetNotFound;

    const comment_start = findCommentStart(line);
    const comment_index = comment_start orelse line.len;

    try writer.writeAll(line[0..rel_ref_start]);
    try writer.writeAll(candidate.next);

    if (shouldWriteVersionComment(candidate)) {
        try writer.writeAll(line[rel_ref_end..comment_index]);
        try writer.writeAll(" # ");
        try writer.writeAll(displayTarget(candidate));
        return;
    }

    try writer.writeAll(line[rel_ref_end..]);
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
    const found = try parse.parseWorkflowString(std.testing.allocator, ".github/workflows/ci.yml", input);
    defer parse.deinitFoundActions(std.testing.allocator, found);

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
            .ref_start = found[0].ref_start,
            .ref_end = found[0].ref_end,
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

test "apply quoted version update without adding comment" {
    const input =
        \\steps:
        \\  - uses: "actions/setup-node@v3"
        \\
    ;
    const found = try parse.parseWorkflowString(std.testing.allocator, "ci.yml", input);
    defer parse.deinitFoundActions(std.testing.allocator, found);

    const candidates = [_]types.Candidate{
        .{
            .action = "actions/setup-node",
            .job = "build",
            .current = "v3",
            .next = "v4",
            .next_label = "v4",
            .file = "ci.yml",
            .line = 2,
            .ref_start = found[0].ref_start,
            .ref_end = found[0].ref_end,
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
    const found = try parse.parseWorkflowString(std.testing.allocator, "ci-security.yml", input);
    defer parse.deinitFoundActions(std.testing.allocator, found);

    const candidates = [_]types.Candidate{
        .{
            .action = "luxass/shared-workflows/.github/workflows/reusable-ci-security.yaml",
            .job = "zizmor",
            .current = "v0.6.0",
            .next = "v0.7.0",
            .next_label = "v0.7.0",
            .file = "ci-security.yml",
            .line = 3,
            .ref_start = found[0].ref_start,
            .ref_end = found[0].ref_end,
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
