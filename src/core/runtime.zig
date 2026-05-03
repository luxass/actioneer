const std = @import("std");

pub const env = struct {
    pub const ci = "CI";
    pub const verbose = "VERBOSE";
};

pub const Environment = struct {
    ci: bool = false,
    verbose: bool = false,

    pub fn from(environ_map: *std.process.Environ.Map, overrides: Overrides) Environment {
        return .{
            .ci = boolEnv(environ_map, env.ci),
            .verbose = overrides.verbose or boolEnv(environ_map, env.verbose),
        };
    }
};

pub const Overrides = struct {
    verbose: bool = false,
};

var current: Environment = .{};

pub fn init(environ_map: *std.process.Environ.Map, overrides: Overrides) void {
    current = Environment.from(environ_map, overrides);
}

pub fn isCi() bool {
    return current.ci;
}

pub fn isVerbose() bool {
    return current.verbose;
}

pub fn boolEnv(environ_map: *std.process.Environ.Map, name: []const u8) bool {
    const value = environ_map.get(name) orelse return false;
    return std.ascii.eqlIgnoreCase(value, "true") or
        std.mem.eql(u8, value, "1") or
        std.ascii.eqlIgnoreCase(value, "yes") or
        std.ascii.eqlIgnoreCase(value, "on");
}
