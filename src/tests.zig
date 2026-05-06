test "module tests" {
    _ = @import("cli/prompt.zig");
    _ = @import("cli/ui.zig");
    _ = @import("core/file_rewriter.zig");
    _ = @import("core/git.zig");
    _ = @import("core/github.zig");
    _ = @import("core/parse.zig");
    _ = @import("syntax/github_actions.zig");
    _ = @import("syntax/yaml_tree.zig");
}
