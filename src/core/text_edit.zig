const std = @import("std");

pub const TextEdit = struct {
    start: usize,
    end: usize,
    replacement: []const u8,
};

pub const ApplyError = error{
    InvalidEditRange,
    OverlappingEdits,
} || std.mem.Allocator.Error;

pub fn applyEdits(allocator: std.mem.Allocator, contents: []const u8, edits: []const TextEdit) ApplyError![]const u8 {
    if (edits.len == 0) return allocator.dupe(u8, contents);

    const sorted = try allocator.dupe(TextEdit, edits);
    defer allocator.free(sorted);

    std.sort.insertion(TextEdit, sorted, {}, lessThanEdit);

    var out = std.ArrayList(u8).empty;
    defer out.deinit(allocator);

    var cursor: usize = 0;
    for (sorted) |edit| {
        if (edit.start > edit.end or edit.end > contents.len) return error.InvalidEditRange;
        if (edit.start < cursor) return error.OverlappingEdits;

        try out.appendSlice(allocator, contents[cursor..edit.start]);
        try out.appendSlice(allocator, edit.replacement);
        cursor = edit.end;
    }

    try out.appendSlice(allocator, contents[cursor..]);
    return out.toOwnedSlice(allocator);
}

fn lessThanEdit(_: void, lhs: TextEdit, rhs: TextEdit) bool {
    return lhs.start < rhs.start;
}
