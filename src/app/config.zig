const std = @import("std");

pub const AppConfig = struct {
    ci: bool = false,
    verbose: bool = false,

    pub fn fromInputs(environ_map: *const std.process.Environ.Map, verbose_override: bool) AppConfig {
        return .{
            .ci = envFlag(environ_map, "CI"),
            .verbose = verbose_override or envFlag(environ_map, "VERBOSE"),
        };
    }
};

fn envFlag(environ_map: *const std.process.Environ.Map, name: []const u8) bool {
    const value = environ_map.get(name) orelse return false;
    return std.ascii.eqlIgnoreCase(value, "true") or
        std.mem.eql(u8, value, "1") or
        std.ascii.eqlIgnoreCase(value, "yes") or
        std.ascii.eqlIgnoreCase(value, "on");
}
