test "module tests" {
    _ = @import("app/ui/prompt.zig");
    _ = @import("app/ui/output.zig");
    _ = @import("core/file_rewriter.zig");
    _ = @import("core/git.zig");
    _ = @import("core/github.zig");
    _ = @import("core/parse.zig");
    _ = @import("syntax/github_actions.zig");
    _ = @import("syntax/yaml_tree.zig");
}
