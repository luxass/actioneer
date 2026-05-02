const std = @import("std");
const builtin = @import("builtin");
const zli = @import("zli");

const style = @import("../core/ui/style.zig");
const text = @import("../core/ui/text.zig");
const updates = @import("../core/updates.zig");

const posix = std.posix;

pub const Error = error{
    UnsupportedPlatform,
    NotATerminal,
    Canceled,
    Interrupted,
} || std.mem.Allocator.Error || std.posix.TermiosGetError || std.posix.TermiosSetError || std.posix.ReadError || std.Io.Writer.Error;

const Key = enum {
    up,
    down,
    scroll_up,
    scroll_down,
    toggle,
    toggle_all,
    toggle_file,
    invert,
    select_none,
    accept,
    cancel,
    ignore,
};

const Widths = struct {
    action: usize,
    job: usize,
    current: usize,
};

const visible_rows = 18;

const RawTerminal = struct {
    original: posix.termios,
    active: bool = false,

    fn enter() !RawTerminal {
        const original = try posix.tcgetattr(posix.STDIN_FILENO);
        var raw = original;

        raw.iflag.ICRNL = false;
        raw.iflag.IXON = false;
        raw.lflag.ECHO = false;
        raw.lflag.ICANON = false;
        raw.lflag.IEXTEN = false;
        raw.cc[@intFromEnum(posix.V.MIN)] = 0;
        raw.cc[@intFromEnum(posix.V.TIME)] = 1;

        try posix.tcsetattr(posix.STDIN_FILENO, .FLUSH, raw);
        return .{ .original = original, .active = true };
    }

    fn deinit(self: *RawTerminal) void {
        if (!self.active) return;
        posix.tcsetattr(posix.STDIN_FILENO, .FLUSH, self.original) catch {};
        self.active = false;
    }
};

pub fn selectUpdates(
    allocator: std.mem.Allocator,
    ctx: zli.CommandContext,
    candidates: []const updates.Candidate,
) Error![]usize {
    if (builtin.os.tag == .windows) return error.UnsupportedPlatform;
    if (candidates.len == 0) return allocator.alloc(usize, 0);
    if (!try std.Io.File.stdin().isTty(ctx.io)) return error.NotATerminal;

    var selected = try allocator.alloc(bool, candidates.len);
    defer allocator.free(selected);
    @memset(selected, false);

    var terminal = RawTerminal.enter() catch |err| switch (err) {
        error.NotATerminal => return error.NotATerminal,
        else => return err,
    };
    defer terminal.deinit();

    try ctx.writer.print(style.hide_cursor, .{});
    defer {
        ctx.writer.print(style.show_cursor, .{}) catch {};
        ctx.writer.flush() catch {};
    }

    var cursor: usize = 0;
    var scroll: usize = 0;
    var rendered_lines: usize = 0;
    defer clearPrompt(ctx, rendered_lines) catch {};

    while (true) {
        adjustScroll(&scroll, cursor, candidates.len);
        try clearPrompt(ctx, rendered_lines);
        rendered_lines = try render(ctx, candidates, selected, cursor, scroll);

        switch (try readKey()) {
            .up => cursor = if (cursor == 0) candidates.len - 1 else cursor - 1,
            .down => cursor = if (cursor + 1 == candidates.len) 0 else cursor + 1,
            .scroll_up => scrollUp(&scroll, &cursor, candidates.len),
            .scroll_down => scrollDown(&scroll, &cursor, candidates.len),
            .toggle => selected[cursor] = !selected[cursor],
            .toggle_all => toggleAll(selected),
            .toggle_file => toggleFile(candidates, selected, candidates[cursor].file),
            .invert => invertSelected(selected),
            .select_none => @memset(selected, false),
            .accept => break,
            .cancel => return error.Canceled,
            .ignore => {},
        }
    }

    var indexes: std.ArrayList(usize) = .empty;
    errdefer indexes.deinit(allocator);

    for (selected, 0..) |is_selected, index| {
        if (is_selected) try indexes.append(allocator, index);
    }

    return indexes.toOwnedSlice(allocator);
}

fn render(
    ctx: zli.CommandContext,
    candidates: []const updates.Candidate,
    selected: []const bool,
    cursor: usize,
    scroll: usize,
) Error!usize {
    var lines: usize = 0;

    try ctx.writer.print("{s}{s}{s}\n\n", .{ style.bold, text.prompt.title, style.reset });
    lines += 2;
    try ctx.writer.print("{s}•{s} {s}{d}{s} update candidates across {s}{d}{s} workflow files\n", .{
        style.green,
        style.reset,
        style.yellow,
        candidates.len,
        style.reset,
        style.yellow,
        workflowCount(candidates),
        style.reset,
    });
    lines += 1;
    try ctx.writer.print("{s}?{s} {s}{s}\n", .{
        style.cyan,
        style.reset,
        text.prompt.controls_summary,
        style.reset,
    });
    lines += 1;

    const widths = Widths{
        .action = maxWidth(candidates, .action),
        .job = maxWidth(candidates, .job),
        .current = maxWidth(candidates, .current),
    };

    const end = @min(candidates.len, scroll + visible_rows);
    if (scroll > 0) {
        try ctx.writer.print("{s}↑ {d} more above{s}\n", .{ style.gray, scroll, style.reset });
        lines += 1;
    }

    var current_file: ?[]const u8 = null;
    for (candidates[scroll..end], scroll..) |candidate, index| {
        if (current_file == null or !std.mem.eql(u8, current_file.?, candidate.file)) {
            current_file = candidate.file;
            lines += try renderGroupHeader(ctx, candidate.file, widths);
        }

        try renderRow(ctx, candidate, selected[index], index == cursor, widths);
        lines += 1;
    }

    if (end < candidates.len) {
        try ctx.writer.print("{s}↓ {d} more below{s}\n", .{ style.gray, candidates.len - end, style.reset });
        lines += 1;
    }

    try ctx.writer.print("\n{s}{s}{s}\n", .{ style.cyan, text.prompt.footer, style.reset });
    lines += 2;
    try ctx.writer.flush();

    return lines;
}

fn renderGroupHeader(ctx: zli.CommandContext, file: []const u8, widths: Widths) Error!usize {
    try ctx.writer.print("\n{s}› {s}{s}\n", .{ style.cyan, file, style.reset });
    try ctx.writer.print("  {s}○ Action{s}", .{ style.gray, style.reset });
    try writePadding(ctx, widths.action - "Action".len + 4);
    try ctx.writer.print("{s}Job{s}", .{ style.gray, style.reset });
    try writePadding(ctx, widths.job - "Job".len + 4);
    try ctx.writer.print("{s}Current{s}", .{ style.gray, style.reset });
    try writePadding(ctx, widths.current - "Current".len + 14);
    try ctx.writer.print("{s}› Target{s}\n", .{ style.gray, style.reset });

    return 3;
}

fn renderRow(
    ctx: zli.CommandContext,
    candidate: updates.Candidate,
    is_selected: bool,
    is_cursor: bool,
    widths: Widths,
) Error!void {
    const row_style = if (is_cursor) style.reverse else style.dim;
    const pointer = if (is_cursor) "›" else " ";
    const checkbox_color = if (is_selected) style.green else style.gray;
    const checkbox = if (is_selected) "●" else "○";
    const target_color = if (candidate.sha_mismatch) style.yellow else if (candidate.next_is_major) style.red else style.green;
    const target = displayTarget(candidate);

    try ctx.writer.print("{s}{s}{s}  {s}{s}{s} {s}{s}{s}{s}", .{
        row_style,
        pointer,
        style.reset,
        checkbox_color,
        checkbox,
        style.reset,
        row_style,
        style.bold,
        candidate.action,
        style.reset,
    });
    try writePadding(ctx, widths.action - candidate.action.len + 4);

    try ctx.writer.print("{s}{s}{s}", .{ row_style, candidate.job, style.reset });
    try writePadding(ctx, widths.job - candidate.job.len + 4);

    try ctx.writer.print("{s}{s}{s}", .{ style.bold, candidate.current, style.reset });
    if (candidate.version_comment.len > 0) {
        const mismatch_color = if (candidate.sha_mismatch) style.red else style.gray;
        try ctx.writer.print(" {s}({s}){s}", .{ mismatch_color, candidate.version_comment, style.reset });
    } else if (candidate.current_ref.len > 0) {
        try ctx.writer.print(" {s}({s}){s}", .{ style.gray, shortSha(candidate.current_ref), style.reset });
    }
    try writePadding(ctx, widths.current - candidate.current.len + 4);

    try ctx.writer.print("{s}›{s}  {s}{s}{s}\n", .{
        style.bold,
        style.reset,
        target_color,
        target,
        style.reset,
    });
}

fn adjustScroll(scroll: *usize, cursor: usize, count: usize) void {
    if (count <= visible_rows) {
        scroll.* = 0;
        return;
    }

    if (cursor < scroll.*) {
        scroll.* = cursor;
        return;
    }

    if (cursor >= scroll.* + visible_rows) {
        scroll.* = cursor + 1 - visible_rows;
    }
}

fn displayTarget(candidate: updates.Candidate) []const u8 {
    return text.displayTarget(candidate);
}

fn clearPrompt(ctx: zli.CommandContext, rendered_lines: usize) Error!void {
    if (rendered_lines == 0) return;

    try ctx.writer.print("\x1b[{d}F\x1b[J", .{rendered_lines});
    try ctx.writer.flush();
}

fn shortSha(sha: []const u8) []const u8 {
    return sha[0..@min(7, sha.len)];
}

fn readKey() Error!Key {
    var byte: [1]u8 = undefined;
    var read_len: usize = 0;
    while (read_len == 0) {
        read_len = try posix.read(posix.STDIN_FILENO, &byte);
    }

    return switch (byte[0]) {
        3 => error.Interrupted,
        4, 27, 'q' => keyOrEscapeSequence(),
        '\r', '\n' => .accept,
        ' ', 'x' => .toggle,
        'a' => .toggle_all,
        'f' => .toggle_file,
        'i' => .invert,
        'n' => .select_none,
        'j' => .down,
        'k' => .up,
        else => .ignore,
    };
}

fn keyOrEscapeSequence() Error!Key {
    var seq: [2]u8 = undefined;
    const len = posix.read(posix.STDIN_FILENO, &seq) catch return .cancel;
    if (len < 2 or seq[0] != '[') return .cancel;

    return switch (seq[1]) {
        'A' => .scroll_up,
        'B' => .scroll_down,
        else => .ignore,
    };
}

fn scrollUp(scroll: *usize, cursor: *usize, count: usize) void {
    if (scroll.* == 0) return;
    scroll.* -= 1;

    if (cursor.* >= scroll.* + @min(visible_rows, count)) {
        cursor.* = scroll.* + @min(visible_rows, count) - 1;
    }
}

fn scrollDown(scroll: *usize, cursor: *usize, count: usize) void {
    if (count <= visible_rows) return;

    const max_scroll = count - visible_rows;
    if (scroll.* >= max_scroll) return;
    scroll.* += 1;

    if (cursor.* < scroll.*) {
        cursor.* = scroll.*;
    }
}

const WidthField = enum { action, job, current };

fn maxWidth(candidates: []const updates.Candidate, field: WidthField) usize {
    var width: usize = 0;
    for (candidates) |candidate| {
        const len = switch (field) {
            .action => candidate.action.len,
            .job => candidate.job.len,
            .current => candidate.current.len,
        };
        width = @max(width, len);
    }
    return width;
}

fn workflowCount(candidates: []const updates.Candidate) usize {
    var count: usize = 0;
    var last_file: ?[]const u8 = null;
    for (candidates) |candidate| {
        if (last_file == null or !std.mem.eql(u8, last_file.?, candidate.file)) {
            count += 1;
            last_file = candidate.file;
        }
    }
    return count;
}

fn invertSelected(selected: []bool) void {
    for (selected) |*item| {
        item.* = !item.*;
    }
}

fn toggleAll(selected: []bool) void {
    var all_selected = true;
    for (selected) |item| {
        if (!item) {
            all_selected = false;
            break;
        }
    }
    @memset(selected, !all_selected);
}

fn toggleFile(candidates: []const updates.Candidate, selected: []bool, file: []const u8) void {
    var all_selected = true;
    for (candidates, 0..) |candidate, index| {
        if (!std.mem.eql(u8, candidate.file, file)) continue;
        if (!selected[index]) {
            all_selected = false;
            break;
        }
    }

    for (candidates, 0..) |candidate, index| {
        if (std.mem.eql(u8, candidate.file, file)) {
            selected[index] = !all_selected;
        }
    }
}

fn writePadding(ctx: zli.CommandContext, count: usize) Error!void {
    var i: usize = 0;
    while (i < count) : (i += 1) {
        try ctx.writer.writeByte(' ');
    }
}
