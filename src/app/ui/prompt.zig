const std = @import("std");
const builtin = @import("builtin");
const zli = @import("zli");

const github = @import("../../core/github.zig");
const output = @import("output.zig");
const styles = @import("styles.zig");

const prompt_text = struct {
    const title = "Choose action updates";
    const controls_summary = "Move selection with arrows or j/k";
    const footer = "Up/Down or j/k move  <space> row  <f> file  <enter> apply  <a> all  <i> invert  <n> none  <q> cancel";
};

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
const header_lines = 4;

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
    candidates: []const github.Candidate,
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

    try ctx.writer.print(styles.HIDE_CURSOR, .{});
    defer {
        ctx.writer.print(styles.SHOW_CURSOR, .{}) catch {};
        ctx.writer.flush() catch {};
    }

    var cursor: usize = 0;
    var scroll: usize = 0;
    var rendered_dynamic_lines: usize = 0;
    defer clearPrompt(ctx, header_lines + rendered_dynamic_lines) catch {};

    try renderHeader(ctx, candidates);

    while (true) {
        adjustScroll(&scroll, cursor, candidates.len);
        try clearPrompt(ctx, rendered_dynamic_lines);
        rendered_dynamic_lines = try renderDynamic(ctx, candidates, selected, cursor, scroll);

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

fn renderHeader(ctx: zli.CommandContext, candidates: []const github.Candidate) Error!void {
    try ctx.writer.print("{s}{s}{s}\n\n", .{ styles.BOLD, prompt_text.title, styles.RESET });
    try ctx.writer.print("{s}•{s} {s}{d}{s} update candidates across {s}{d}{s} workflow files\n", .{
        styles.GREEN,
        styles.RESET,
        styles.YELLOW,
        candidates.len,
        styles.RESET,
        styles.YELLOW,
        workflowCount(candidates),
        styles.RESET,
    });
    try ctx.writer.print("{s}?{s} {s}{s}\n", .{
        styles.CYAN,
        styles.RESET,
        prompt_text.controls_summary,
        styles.RESET,
    });
    try ctx.writer.flush();
}

fn renderDynamic(
    ctx: zli.CommandContext,
    candidates: []const github.Candidate,
    selected: []const bool,
    cursor: usize,
    scroll: usize,
) Error!usize {
    var lines: usize = 0;

    const widths = Widths{
        .action = maxWidth(candidates, .action),
        .job = maxWidth(candidates, .job),
        .current = maxWidth(candidates, .current),
    };

    const end = @min(candidates.len, scroll + visible_rows);
    if (scroll > 0) {
        try ctx.writer.print("{s}↑ {d} more above{s}\n", .{ styles.BRIGHT_BLACK, scroll, styles.RESET });
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
        try ctx.writer.print("{s}↓ {d} more below{s}\n", .{ styles.BRIGHT_BLACK, candidates.len - end, styles.RESET });
        lines += 1;
    }

    try ctx.writer.print("\n{s}{s}{s}\n", .{ styles.CYAN, prompt_text.footer, styles.RESET });
    lines += 2;
    try ctx.writer.flush();

    return lines;
}

fn renderGroupHeader(ctx: zli.CommandContext, file: []const u8, widths: Widths) Error!usize {
    try ctx.writer.print("\n{s}› {s}{s}\n", .{ styles.CYAN, file, styles.RESET });
    try ctx.writer.print("  {s}○ Action{s}", .{ styles.BRIGHT_BLACK, styles.RESET });
    try writePadding(ctx, trailingPadding(widths.action, "Action".len, 4));
    try ctx.writer.print("{s}Job{s}", .{ styles.BRIGHT_BLACK, styles.RESET });
    try writePadding(ctx, trailingPadding(widths.job, "Job".len, 4));
    try ctx.writer.print("{s}Current{s}", .{ styles.BRIGHT_BLACK, styles.RESET });
    try writePadding(ctx, trailingPadding(widths.current, "Current".len, 14));
    try ctx.writer.print("{s}› Target{s}\n", .{ styles.BRIGHT_BLACK, styles.RESET });

    return 3;
}

fn renderRow(
    ctx: zli.CommandContext,
    row_candidate: github.Candidate,
    is_selected: bool,
    is_cursor: bool,
    widths: Widths,
) Error!void {
    const row_style = if (is_cursor) styles.INVERSE else styles.DIM;
    const pointer = if (is_cursor) "›" else " ";
    const checkbox_color = if (is_selected) styles.GREEN else styles.BRIGHT_BLACK;
    const checkbox = if (is_selected) "●" else "○";
    const target_color = if (row_candidate.hasShaMismatch()) styles.YELLOW else if (row_candidate.isMajorUpdate()) styles.RED else styles.GREEN;
    const target = row_candidate.displayTarget();

    try ctx.writer.print("{s}{s}{s}  {s}{s}{s} {s}{s}{s}{s}", .{
        row_style,
        pointer,
        styles.RESET,
        checkbox_color,
        checkbox,
        styles.RESET,
        row_style,
        styles.BOLD,
        row_candidate.action,
        styles.RESET,
    });
    try writePadding(ctx, trailingPadding(widths.action, row_candidate.action.len, 4));

    try ctx.writer.print("{s}{s}{s}", .{ row_style, row_candidate.job, styles.RESET });
    try writePadding(ctx, trailingPadding(widths.job, row_candidate.job.len, 4));

    try ctx.writer.print("{s}{s}{s}", .{ styles.BOLD, row_candidate.current, styles.RESET });
    if (row_candidate.hasVersionComment()) {
        const mismatch_color = if (row_candidate.hasShaMismatch()) styles.RED else styles.BRIGHT_BLACK;
        try ctx.writer.print(" {s}({s}){s}", .{ mismatch_color, row_candidate.version_comment, styles.RESET });
    } else if (row_candidate.hasCurrentRef()) {
        try ctx.writer.print(" {s}({s}){s}", .{ styles.BRIGHT_BLACK, shortSha(row_candidate.current_ref), styles.RESET });
    }
    try writePadding(ctx, trailingPadding(widths.current, row_candidate.current.len, 4));

    try ctx.writer.print("{s}›{s}  {s}{s}{s}\n", .{
        styles.BOLD,
        styles.RESET,
        target_color,
        target,
        styles.RESET,
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
        4, 'q' => .cancel,
        27 => keyOrEscapeSequence(),
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
        'A' => .up,
        'B' => .down,
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

fn maxWidth(candidates: []const github.Candidate, field: WidthField) usize {
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

fn trailingPadding(column_width: usize, value_len: usize, gap: usize) usize {
    return column_width -| value_len + gap;
}

fn workflowCount(candidates: []const github.Candidate) usize {
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

fn toggleFile(candidates: []const github.Candidate, selected: []bool, file: []const u8) void {
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

test "trailing padding does not underflow for header labels wider than values" {
    try std.testing.expectEqual(@as(usize, 14), trailingPadding(2, "Current".len, 14));
    try std.testing.expectEqual(@as(usize, 4), trailingPadding(3, "Action".len, 4));
    try std.testing.expectEqual(@as(usize, 11), trailingPadding(10, "Job".len, 4));
}
