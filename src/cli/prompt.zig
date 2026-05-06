const std = @import("std");
const builtin = @import("builtin");
const zli = @import("zli");

const types = @import("../core/types.zig");
const ui = @import("ui.zig");

const styles = ui.styles;
const text = ui;

const posix = std.posix;
const windows = std.os.windows;
const is_windows = builtin.os.tag == .windows;

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

const RawTerminal = if (is_windows) WindowsTerminal else PosixTerminal;

const InputSource = union(enum) {
    live,
    scripted: ScriptedInput,

    fn next(self: *InputSource) Error!Key {
        return switch (self.*) {
            .live => readKey(),
            .scripted => |*scripted| scripted.next(),
        };
    }
};

const ScriptedInput = struct {
    keys: []const Key,
    index: usize = 0,

    fn next(self: *ScriptedInput) Error!Key {
        if (self.index >= self.keys.len) return .accept;
        const key = self.keys[self.index];
        self.index += 1;
        return key;
    }
};

const WindowsTerminal = struct {
    fn enter(io: std.Io) !WindowsTerminal {
        std.Io.File.stdout().enableAnsiEscapeCodes(io) catch |err| switch (err) {
            error.NotTerminalDevice => return error.NotATerminal,
            else => return error.UnsupportedPlatform,
        };
        return .{};
    }

    fn deinit(_: *WindowsTerminal) void {}
};

const PosixTerminal = struct {
    original: posix.termios,
    active: bool = false,

    fn enter(_: std.Io) !PosixTerminal {
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

    fn deinit(self: *PosixTerminal) void {
        if (!self.active) return;
        posix.tcsetattr(posix.STDIN_FILENO, .FLUSH, self.original) catch {};
        self.active = false;
    }
};

pub fn selectUpdates(
    allocator: std.mem.Allocator,
    ctx: zli.CommandContext,
    candidates: []const types.Candidate,
) Error![]usize {
    if (candidates.len == 0) return allocator.alloc(usize, 0);
    if (!try std.Io.File.stdin().isTty(ctx.io)) return error.NotATerminal;

    var terminal = try RawTerminal.enter(ctx.io);
    defer terminal.deinit();

    try ctx.writer.print(styles.HIDE_CURSOR, .{});
    defer {
        ctx.writer.print(styles.SHOW_CURSOR, .{}) catch {};
        ctx.writer.flush() catch {};
    }

    var input: InputSource = .live;
    return selectUpdatesWithInput(allocator, ctx.writer, candidates, &input);
}

fn selectUpdatesWithInput(
    allocator: std.mem.Allocator,
    writer: *std.Io.Writer,
    candidates: []const types.Candidate,
    input: *InputSource,
) Error![]usize {
    const selected = try allocator.alloc(bool, candidates.len);
    defer allocator.free(selected);
    @memset(selected, false);

    var cursor: usize = 0;
    var scroll: usize = 0;
    var rendered_dynamic_lines: usize = 0;
    defer clearPrompt(writer, header_lines + rendered_dynamic_lines) catch {};

    try renderHeader(writer, candidates);

    while (true) {
        adjustScroll(&scroll, cursor, candidates.len);
        try clearPrompt(writer, rendered_dynamic_lines);
        rendered_dynamic_lines = try renderDynamic(writer, candidates, selected, cursor, scroll);

        switch (try input.next()) {
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

fn renderHeader(writer: *std.Io.Writer, candidates: []const types.Candidate) Error!void {
    try writer.print("{s}{s}{s}\n\n", .{ styles.BOLD, text.prompt.title, styles.RESET });
    try writer.print("{s}•{s} {s}{d}{s} update candidates across {s}{d}{s} workflow files\n", .{
        styles.GREEN,
        styles.RESET,
        styles.YELLOW,
        candidates.len,
        styles.RESET,
        styles.YELLOW,
        workflowCount(candidates),
        styles.RESET,
    });
    try writer.print("{s}?{s} {s}{s}\n", .{
        styles.CYAN,
        styles.RESET,
        text.prompt.controls_summary,
        styles.RESET,
    });
    try writer.flush();
}

fn renderDynamic(
    writer: *std.Io.Writer,
    candidates: []const types.Candidate,
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
        try writer.print("{s}↑ {d} more above{s}\n", .{ styles.BRIGHT_BLACK, scroll, styles.RESET });
        lines += 1;
    }

    var current_file: ?[]const u8 = null;
    for (candidates[scroll..end], scroll..) |candidate, index| {
        if (current_file == null or !std.mem.eql(u8, current_file.?, candidate.file)) {
            current_file = candidate.file;
            lines += try renderGroupHeader(writer, candidate.file, widths);
        }

        try renderRow(writer, candidate, selected[index], index == cursor, widths);
        lines += 1;
    }

    if (end < candidates.len) {
        try writer.print("{s}↓ {d} more below{s}\n", .{ styles.BRIGHT_BLACK, candidates.len - end, styles.RESET });
        lines += 1;
    }

    try writer.print("\n{s}{s}{s}\n", .{ styles.CYAN, text.prompt.footer, styles.RESET });
    lines += 2;
    try writer.flush();

    return lines;
}

fn renderGroupHeader(writer: *std.Io.Writer, file: []const u8, widths: Widths) Error!usize {
    try writer.print("\n{s}› {s}{s}\n", .{ styles.CYAN, file, styles.RESET });
    try writer.print("  {s}○ Action{s}", .{ styles.BRIGHT_BLACK, styles.RESET });
    try writePadding(writer, trailingPadding(widths.action, "Action".len, 4));
    try writer.print("{s}Job{s}", .{ styles.BRIGHT_BLACK, styles.RESET });
    try writePadding(writer, trailingPadding(widths.job, "Job".len, 4));
    try writer.print("{s}Current{s}", .{ styles.BRIGHT_BLACK, styles.RESET });
    try writePadding(writer, trailingPadding(widths.current, "Current".len, 14));
    try writer.print("{s}› Target{s}\n", .{ styles.BRIGHT_BLACK, styles.RESET });

    return 3;
}

fn renderRow(
    writer: *std.Io.Writer,
    candidate: types.Candidate,
    is_selected: bool,
    is_cursor: bool,
    widths: Widths,
) Error!void {
    const row_style = if (is_cursor) styles.INVERSE else styles.DIM;
    const pointer = if (is_cursor) "›" else " ";
    const checkbox_color = if (is_selected) styles.GREEN else styles.BRIGHT_BLACK;
    const checkbox = if (is_selected) "●" else "○";
    const target_color = if (candidate.sha_mismatch) styles.YELLOW else if (candidate.next_is_major) styles.RED else styles.GREEN;
    const target = displayTarget(candidate);

    try writer.print("{s}{s}{s}  {s}{s}{s} {s}{s}{s}{s}", .{
        row_style,
        pointer,
        styles.RESET,
        checkbox_color,
        checkbox,
        styles.RESET,
        row_style,
        styles.BOLD,
        candidate.action,
        styles.RESET,
    });
    try writePadding(writer, trailingPadding(widths.action, candidate.action.len, 4));

    try writer.print("{s}{s}{s}", .{ row_style, candidate.job, styles.RESET });
    try writePadding(writer, trailingPadding(widths.job, candidate.job.len, 4));

    try writer.print("{s}{s}{s}", .{ styles.BOLD, candidate.current, styles.RESET });
    if (candidate.version_comment.len > 0) {
        const mismatch_color = if (candidate.sha_mismatch) styles.RED else styles.BRIGHT_BLACK;
        try writer.print(" {s}({s}){s}", .{ mismatch_color, candidate.version_comment, styles.RESET });
    } else if (candidate.current_ref.len > 0) {
        try writer.print(" {s}({s}){s}", .{ styles.BRIGHT_BLACK, shortSha(candidate.current_ref), styles.RESET });
    }
    try writePadding(writer, trailingPadding(widths.current, candidate.current.len, 4));

    try writer.print("{s}›{s}  {s}{s}{s}\n", .{
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

fn displayTarget(candidate: types.Candidate) []const u8 {
    return text.displayTarget(candidate);
}

fn clearPrompt(writer: *std.Io.Writer, rendered_lines: usize) Error!void {
    if (rendered_lines == 0) return;

    try writer.print("\x1b[{d}F\x1b[J", .{rendered_lines});
    try writer.flush();
}

fn shortSha(sha: []const u8) []const u8 {
    return sha[0..@min(7, sha.len)];
}

fn readKey() Error!Key {
    if (is_windows) return readWindowsKey();

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

fn readWindowsKey() Error!Key {
    const first = win32Getwch();
    if (first == 0 or first == 0xe0) return decodeWindowsKey(first, win32Getwch());
    return decodeWindowsKey(first, null);
}

fn decodeWindowsKey(first: u16, second: ?u16) Error!Key {
    if (first == 0 or first == 0xe0) {
        return switch (second orelse return .ignore) {
            72 => .up,
            73 => .scroll_up,
            80 => .down,
            81 => .scroll_down,
            else => .ignore,
        };
    }

    return switch (first) {
        3 => error.Interrupted,
        4, 'q', 27 => .cancel,
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

fn maxWidth(candidates: []const types.Candidate, field: WidthField) usize {
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

fn workflowCount(candidates: []const types.Candidate) usize {
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

fn toggleFile(candidates: []const types.Candidate, selected: []bool, file: []const u8) void {
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

fn writePadding(writer: *std.Io.Writer, count: usize) Error!void {
    var i: usize = 0;
    while (i < count) : (i += 1) {
        try writer.writeByte(' ');
    }
}

fn win32Getwch() u16 {
    if (!is_windows) unreachable;

    const Conio = struct {
        extern "c" fn _getwch() callconv(.c) windows.WCHAR;
    };

    return Conio._getwch();
}

test "trailing padding does not underflow for header labels wider than values" {
    try std.testing.expectEqual(@as(usize, 14), trailingPadding(2, "Current".len, 14));
    try std.testing.expectEqual(@as(usize, 4), trailingPadding(3, "Action".len, 4));
    try std.testing.expectEqual(@as(usize, 11), trailingPadding(10, "Job".len, 4));
}

test "scripted picker flow renders and selects expected updates" {
    const candidates = [_]types.Candidate{
        .{
            .action = "actions/checkout",
            .job = "build",
            .current = "v4",
            .next = "v5",
            .next_label = "v5",
            .file = ".github/workflows/ci.yml",
            .line = 4,
            .ref_start = 0,
            .ref_end = 0,
        },
        .{
            .action = "actions/setup-node",
            .job = "build",
            .current = "v4",
            .next = "v5",
            .next_label = "v5",
            .file = ".github/workflows/ci.yml",
            .line = 5,
            .ref_start = 0,
            .ref_end = 0,
        },
    };

    var out = std.Io.Writer.Allocating.init(std.testing.allocator);
    defer out.deinit();

    var input: InputSource = .{ .scripted = .{
        .keys = &.{ .toggle, .down, .toggle, .accept },
    } };

    const selected = try selectUpdatesWithInput(std.testing.allocator, &out.writer, &candidates, &input);
    defer std.testing.allocator.free(selected);

    try std.testing.expectEqualSlices(usize, &.{ 0, 1 }, selected);
    try std.testing.expect(std.mem.indexOf(u8, out.written(), text.prompt.title) != null);
    try std.testing.expect(std.mem.indexOf(u8, out.written(), "actions/checkout") != null);
    try std.testing.expect(std.mem.indexOf(u8, out.written(), "actions/setup-node") != null);
    try std.testing.expect(std.mem.indexOf(u8, out.written(), "\x1b[") != null);
}

test "scripted picker can toggle file selection" {
    const candidates = [_]types.Candidate{
        .{
            .action = "actions/checkout",
            .job = "build",
            .current = "v4",
            .next = "v5",
            .next_label = "v5",
            .file = ".github/workflows/ci.yml",
            .line = 4,
            .ref_start = 0,
            .ref_end = 0,
        },
        .{
            .action = "actions/setup-node",
            .job = "build",
            .current = "v4",
            .next = "v5",
            .next_label = "v5",
            .file = ".github/workflows/ci.yml",
            .line = 5,
            .ref_start = 0,
            .ref_end = 0,
        },
        .{
            .action = "owner/repo",
            .job = "lint",
            .current = "v1",
            .next = "v2",
            .next_label = "v2",
            .file = ".github/workflows/other.yml",
            .line = 7,
            .ref_start = 0,
            .ref_end = 0,
        },
    };

    var out = std.Io.Writer.Allocating.init(std.testing.allocator);
    defer out.deinit();

    var input: InputSource = .{ .scripted = .{
        .keys = &.{ .toggle_file, .accept },
    } };

    const selected = try selectUpdatesWithInput(std.testing.allocator, &out.writer, &candidates, &input);
    defer std.testing.allocator.free(selected);

    try std.testing.expectEqualSlices(usize, &.{ 0, 1 }, selected);
}

test "decode windows key maps arrows and controls" {
    try std.testing.expectEqual(Key.up, try decodeWindowsKey(0xe0, 72));
    try std.testing.expectEqual(Key.down, try decodeWindowsKey(0xe0, 80));
    try std.testing.expectEqual(Key.scroll_up, try decodeWindowsKey(0xe0, 73));
    try std.testing.expectEqual(Key.scroll_down, try decodeWindowsKey(0xe0, 81));
    try std.testing.expectEqual(Key.toggle, try decodeWindowsKey(' ', null));
    try std.testing.expectEqual(Key.cancel, try decodeWindowsKey('q', null));
    try std.testing.expectEqual(Key.accept, try decodeWindowsKey('\r', null));
}
