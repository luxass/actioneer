const std = @import("std");
const config = @import("../app/config.zig");

var current: config.AppConfig = .{};

pub fn init(app_config: config.AppConfig) void {
    current = app_config;
}

pub fn isCi() bool {
    return current.ci;
}

pub fn isVerbose() bool {
    return current.verbose;
}
