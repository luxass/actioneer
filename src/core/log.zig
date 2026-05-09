const std = @import("std");

const styles = @import("../app/ui/styles.zig");

var verbose_enabled: bool = false;

pub fn init(enabled: bool) void {
    verbose_enabled = enabled;
}

pub fn debug(comptime fmt: []const u8, args: anytype) void {
    if (!verbose_enabled) return;
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
