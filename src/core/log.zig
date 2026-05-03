const std = @import("std");

const runtime = @import("runtime.zig");

const styles = struct {
    const RESET = "\x1b[0m";
    const BLUE = "\x1b[34m";
    const GREEN = "\x1b[32m";
    const YELLOW = "\x1b[33m";
    const RED = "\x1b[31m";
};

pub fn debug(comptime fmt: []const u8, args: anytype) void {
    if (!runtime.isVerbose()) return;
    write(styles.BLUE, "debug", fmt, args);
}

pub fn info(comptime fmt: []const u8, args: anytype) void {
    write(styles.GREEN, "info", fmt, args);
}

pub fn warn(comptime fmt: []const u8, args: anytype) void {
    write(styles.YELLOW, "warn", fmt, args);
}

pub fn err(comptime fmt: []const u8, args: anytype) void {
    write(styles.RED, "error", fmt, args);
}

pub const @"error" = err;

fn write(comptime color: []const u8, comptime level: []const u8, comptime fmt: []const u8, args: anytype) void {
    std.debug.print(color ++ "[" ++ level ++ "] " ++ styles.RESET ++ fmt ++ "\n", args);
}
