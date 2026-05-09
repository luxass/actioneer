const std = @import("std");

var verbose: bool = false;

pub fn init(enabled: bool) void {
    verbose = enabled;
}

pub fn isVerbose() bool {
    return verbose;
}
