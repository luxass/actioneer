pub const Config = struct {
    dirs: []const []const u8,
    excludes: []const []const u8,
    include_branches: bool,
    mode: []const u8,
    style: []const u8,
    recursive: bool,
    github_token: ?[]const u8 = null,
};
