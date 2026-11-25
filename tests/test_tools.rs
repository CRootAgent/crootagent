use lontar::tools::*;
use serde_json::json;
use tempfile::tempdir;

#[tokio::test]
async fn test_write_and_read_file() {
    let dir = tempdir().expect("Failed to create tempdir");
    let file_path = dir.path().join("sample.txt");
    let file_path_str = file_path.to_str().unwrap();

    let write_tool = WriteFileTool;
    let write_args = json!({
        "path": file_path_str,
        "content": "Line 1\nLine 2\nLine 3\nLine 4\nLine 5"
    });

    let res = write_tool.execute(&write_args).await;
    assert!(res.is_ok(), "WriteFileTool failed: {:?}", res);

    let read_tool = ReadFileTool;
    let read_args = json!({
        "path": file_path_str,
        "start_line": 2,
        "end_line": 4
    });

    let read_res = read_tool.execute(&read_args).await.expect("ReadFileTool failed");
    assert!(read_res.contains("Line 2"));
    assert!(read_res.contains("Line 3"));
    assert!(read_res.contains("Line 4"));
    assert!(!read_res.contains("Line 1"));
    assert!(!read_res.contains("Line 5"));
}

#[tokio::test]
async fn test_edit_file() {
    let dir = tempdir().expect("Failed to create tempdir");
    let file_path = dir.path().join("code.rs");
    let file_path_str = file_path.to_str().unwrap();

    std::fs::write(&file_path, "fn main() {\n    let x = 10;\n}").unwrap();

    let edit_tool = EditFileTool;
    let edit_args = json!({
        "path": file_path_str,
        "target": "let x = 10;",
        "replacement": "let x = 42;"
    });

    let edit_res = edit_tool.execute(&edit_args).await;
    assert!(edit_res.is_ok());

    let content = std::fs::read_to_string(&file_path).unwrap();
    assert!(content.contains("let x = 42;"));
    assert!(!content.contains("let x = 10;"));
}

#[tokio::test]
async fn test_list_dir() {
    let dir = tempdir().expect("Failed to create tempdir");
    std::fs::write(dir.path().join("a.txt"), "hello").unwrap();
    std::fs::create_dir(dir.path().join("sub")).unwrap();

    let tool = ListDirTool;
    let args = json!({
        "path": dir.path().to_str().unwrap(),
        "max_depth": 2
    });

    let res = tool.execute(&args).await.expect("ListDir failed");
    assert!(res.contains("a.txt"));
    assert!(res.contains("sub/"));
}

#[tokio::test]
async fn test_run_command() {
    let tool = RunCommandTool;
    let args = json!({
        "command": "echo 'Lontar CLI Rocks'"
    });

    let res = tool.execute(&args).await.expect("RunCommand failed");
    assert!(res.contains("Exit Code: 0"));
    assert!(res.contains("Lontar CLI Rocks"));
}

#[test]
fn test_tool_registry() {
    let registry = ToolRegistry::default();
    assert!(registry.is_destructive("run_command"));
    assert!(registry.is_destructive("write_file"));
    assert!(registry.is_destructive("edit_file"));
    assert!(!registry.is_destructive("read_file"));
    assert!(!registry.is_destructive("list_dir"));

    let defs = registry.definitions();
    assert_eq!(defs.len(), 5);
}
