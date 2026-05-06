const std = @import("std");

pub const Version = struct {
    major: u32,
    minor: u32,
    patch: u32,

    pub fn order(lhs: Version, rhs: Version) std.math.Order {
        if (lhs.major != rhs.major) return std.math.order(lhs.major, rhs.major);
        if (lhs.minor != rhs.minor) return std.math.order(lhs.minor, rhs.minor);
        return std.math.order(lhs.patch, rhs.patch);
    }
};

pub fn isLikelySha(value: []const u8) bool {
    if (value.len < 7 or value.len > 40) return false;
    for (value) |char| {
        if (!std.ascii.isHex(char)) return false;
    }
    return true;
}

pub fn shaMatches(actual: []const u8, expected: []const u8) bool {
    return std.mem.eql(u8, actual, expected) or std.mem.startsWith(u8, expected, actual);
}

pub fn parseVersion(ref: []const u8) ?Version {
    var value = ref;
    if (value.len > 0 and (value[0] == 'v' or value[0] == 'V')) {
        value = value[1..];
    }
    if (value.len == 0 or !std.ascii.isDigit(value[0])) return null;

    var parts = std.mem.splitScalar(u8, value, '.');
    const major_raw = parts.next() orelse return null;
    const minor_raw = parts.next() orelse "0";
    const patch_raw = parts.next() orelse "0";

    return .{
        .major = parseLeadingInt(major_raw) orelse return null,
        .minor = parseLeadingInt(minor_raw) orelse return null,
        .patch = parseLeadingInt(patch_raw) orelse return null,
    };
}

fn parseLeadingInt(value: []const u8) ?u32 {
    var end: usize = 0;
    while (end < value.len and std.ascii.isDigit(value[end])) : (end += 1) {}
    if (end == 0) return null;
    return std.fmt.parseInt(u32, value[0..end], 10) catch null;
}

test "detect likely git sha" {
    try std.testing.expect(isLikelySha("123abcd"));
    try std.testing.expect(isLikelySha("0123456789abcdef0123456789abcdef01234567"));
    try std.testing.expect(!isLikelySha("123abc"));
    try std.testing.expect(!isLikelySha("not-a-sha"));
    try std.testing.expect(!isLikelySha("0123456789abcdef0123456789abcdef012345678"));
}

test "match full and short shas" {
    try std.testing.expect(shaMatches("123abcd", "123abcdef"));
    try std.testing.expect(shaMatches("123abcdef", "123abcdef"));
    try std.testing.expect(!shaMatches("123abce", "123abcdef"));
}

test "parse version refs" {
    try std.testing.expectEqual(Version{ .major = 1, .minor = 2, .patch = 3 }, parseVersion("v1.2.3").?);
    try std.testing.expectEqual(Version{ .major = 4, .minor = 0, .patch = 0 }, parseVersion("v4").?);
    try std.testing.expectEqual(Version{ .major = 1, .minor = 2, .patch = 3 }, parseVersion("1.2.3-beta").?);
    try std.testing.expect(parseVersion("main") == null);
}
