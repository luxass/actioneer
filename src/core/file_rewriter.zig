const std = @import("std");

const github = @import("github.zig");
const parse = @import("parse.zig");
const text_edit = @import("text_edit.zig");

pub const RewriteError = error{
    UpdateTargetNotFound,
} || text_edit.ApplyError || std.mem.Allocator.Error || std.Io.Dir.ReadFileAllocError || std.Io.Dir.WriteFileError || std.Io.Writer.Error;

pub const RewriteResult = struct {
    contents: []const u8,
    applied: usize,
};

pub fn rewriteSelectedFiles(
    allocator: std.mem.Allocator,
    io: std.Io,
    candidates: []const github.Candidate,
    selected: []const usize,
) RewriteError!usize {
    var applied: usize = 0;
    var handled_files: std.StringHashMap(void) = .init(allocator);
    defer handled_files.deinit();

    for (selected) |selected_index| {
        const file = candidates[selected_index].file;
        if (handled_files.contains(file)) continue;

        const contents = try std.Io.Dir.cwd().readFileAlloc(io, file, allocator, .limited(10 * 1024 * 1024));
        defer allocator.free(contents);

        const result = try rewriteString(allocator, contents, file, candidates, selected);
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

pub fn rewriteString(
    allocator: std.mem.Allocator,
    contents: []const u8,
    file: []const u8,
    candidates: []const github.Candidate,
    selected: []const usize,
) RewriteError!RewriteResult {
    const file_candidates = try selectedCandidatesForFile(allocator, candidates, selected, file);
    defer allocator.free(file_candidates);

    var edits = std.ArrayList(text_edit.TextEdit).empty;
    defer edits.deinit(allocator);
    var owned_replacements = std.ArrayList([]const u8).empty;
    defer {
        for (owned_replacements.items) |replacement| allocator.free(replacement);
        owned_replacements.deinit(allocator);
    }

    for (file_candidates) |candidate| {
        if (candidate.ref_start > candidate.ref_end or candidate.ref_end > contents.len) {
            return error.UpdateTargetNotFound;
        }
        if (!std.mem.eql(u8, contents[candidate.ref_start..candidate.ref_end], candidate.current)) {
            return error.UpdateTargetNotFound;
        }
        try appendEditsForCandidate(allocator, contents, candidate, &edits, &owned_replacements);
    }

    const rewritten = try text_edit.applyEdits(allocator, contents, edits.items);

    return .{
        .contents = rewritten,
        .applied = file_candidates.len,
    };
}

fn selectedCandidatesForFile(
    allocator: std.mem.Allocator,
    candidates: []const github.Candidate,
    selected: []const usize,
    file: []const u8,
) ![]github.Candidate {
    var filtered: std.ArrayList(github.Candidate) = .empty;
    defer filtered.deinit(allocator);

    for (selected) |index| {
        const candidate = candidates[index];
        if (!std.mem.eql(u8, candidate.file, file)) continue;
        try filtered.append(allocator, candidate);
    }

    std.sort.insertion(github.Candidate, filtered.items, {}, lessThanCandidate);
    return filtered.toOwnedSlice(allocator);
}

fn lessThanCandidate(_: void, lhs: github.Candidate, rhs: github.Candidate) bool {
    return lhs.ref_start < rhs.ref_start;
}

fn appendEditsForCandidate(
    allocator: std.mem.Allocator,
    contents: []const u8,
    update_candidate: github.Candidate,
    edits: *std.ArrayList(text_edit.TextEdit),
    owned_replacements: *std.ArrayList([]const u8),
) RewriteError!void {
    try edits.append(allocator, .{
        .start = update_candidate.ref_start,
        .end = update_candidate.ref_end,
        .replacement = update_candidate.next,
    });

    if (!update_candidate.shouldWriteVersionComment()) return;

    const comment_start = findCommentStart(contents, update_candidate.ref_end);
    const line_end = std.mem.indexOfScalarPos(u8, contents, update_candidate.ref_end, '\n') orelse contents.len;
    const comment_range_start = if (comment_start) |start| blk: {
        var replacement_start = start;
        while (replacement_start > update_candidate.ref_end and isHorizontalSpace(contents[replacement_start - 1])) {
            replacement_start -= 1;
        }
        break :blk replacement_start;
    } else line_end;
    const replacement = try std.fmt.allocPrint(allocator, " # {s}", .{update_candidate.displayTarget()});
    errdefer allocator.free(replacement);
    try owned_replacements.append(allocator, replacement);

    try edits.append(allocator, .{
        .start = comment_range_start,
        .end = line_end,
        .replacement = replacement,
    });
}

fn isHorizontalSpace(char: u8) bool {
    return char == ' ' or char == '\t';
}

fn findCommentStart(contents: []const u8, offset: usize) ?usize {
    const line_end = std.mem.indexOfScalarPos(u8, contents, offset, '\n') orelse contents.len;
    const line_start = if (std.mem.lastIndexOfScalar(u8, contents[0..offset], '\n')) |index| index + 1 else 0;
    var quote: ?u8 = null;
    for (contents[line_start..line_end], line_start..) |char, index| {
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

    const candidates = [_]github.Candidate{
        .{
            .action = "actions/checkout",
            .job = "build",
            .current = "oldsha",
            .version_comment = "v4.1.0",
            .next = "newsha",
            .next_label = "v4.2.0",
            .file = ".github/workflows/ci.yml",
            .line = 4,
            .ref_start = found[0].source.ref_span.start,
            .ref_end = found[0].source.ref_span.end,
        },
    };

    const result = try rewriteString(std.testing.allocator, input, ".github/workflows/ci.yml", &candidates, &.{0});
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
        \\jobs:
        \\  build:
        \\    steps:
        \\      - uses: "actions/setup-node@v3"
        \\
    ;
    const found = try parse.parseWorkflowString(std.testing.allocator, "ci.yml", input);
    defer parse.deinitFoundActions(std.testing.allocator, found);

    const candidates = [_]github.Candidate{
        .{
            .action = "actions/setup-node",
            .job = "build",
            .current = "v3",
            .next = "v4",
            .next_label = "v4",
            .file = "ci.yml",
            .line = 2,
            .ref_start = found[0].source.ref_span.start,
            .ref_end = found[0].source.ref_span.end,
        },
    };

    const result = try rewriteString(std.testing.allocator, input, "ci.yml", &candidates, &.{0});
    defer std.testing.allocator.free(result.contents);

    try std.testing.expectEqualStrings(
        \\jobs:
        \\  build:
        \\    steps:
        \\      - uses: "actions/setup-node@v4"
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

    const candidates = [_]github.Candidate{
        .{
            .action = "luxass/shared-workflows/.github/workflows/reusable-ci-security.yaml",
            .job = "zizmor",
            .current = "v0.6.0",
            .next = "v0.7.0",
            .next_label = "v0.7.0",
            .file = "ci-security.yml",
            .line = 3,
            .ref_start = found[0].source.ref_span.start,
            .ref_end = found[0].source.ref_span.end,
        },
    };

    const result = try rewriteString(std.testing.allocator, input, "ci-security.yml", &candidates, &.{0});
    defer std.testing.allocator.free(result.contents);

    try std.testing.expectEqualStrings(
        \\jobs:
        \\  zizmor:
        \\    uses: luxass/shared-workflows/.github/workflows/reusable-ci-security.yaml@v0.7.0
        \\
    , result.contents);
}
