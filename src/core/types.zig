pub const FoundAction = struct {
    action: []const u8,
    owner: []const u8,
    repo: []const u8,
    ref: []const u8,
    version_comment: []const u8 = "",
    job: []const u8,
    file: []const u8,
    line: u32,
};
