use procalc_engine::{Answer, Curve, EngineError, FigureInput, FigureResult, Settings, View};

/// Evaluates one expression (MathLive LaTeX or plain text) with the given settings,
/// after the earlier definition lines in `defs` (oldest first).
#[tauri::command]
fn evaluate(input: &str, defs: Vec<String>, settings: Settings) -> Result<Answer, EngineError> {
    procalc_engine::evaluate_with(input, &defs, &settings)
}

/// Evaluates every line of the Paper in order; definitions carry down the sheet.
#[tauri::command]
fn evaluate_sheet(lines: Vec<String>, settings: Settings) -> Vec<Result<Answer, EngineError>> {
    procalc_engine::evaluate_sheet(&lines, &settings)
}

/// Samples every plottable line of the Paper over the graph's view.
#[tauri::command]
fn plot_sheet(lines: Vec<String>, settings: Settings, view: View) -> Vec<Option<Curve>> {
    procalc_engine::plot_sheet(&lines, &settings, view)
}

/// Solves the geometry workspace's figure: to scale, every part, and its quantities.
#[tauri::command]
fn solve_figure(figure: FigureInput, settings: Settings) -> FigureResult {
    procalc_engine::solve_figure(&figure, &settings)
}

/// Reads a project file (.pcmath, …) chosen in the Open dialog.
#[tauri::command]
fn read_project(path: String) -> Result<String, String> {
    std::fs::read_to_string(&path).map_err(|e| format!("Couldn't open {path}: {e}"))
}

/// Writes a project file, through a temporary file so a failed save can't leave half a file.
#[tauri::command]
fn write_project(path: String, contents: String) -> Result<(), String> {
    let tmp = format!("{path}.tmp");
    std::fs::write(&tmp, contents).map_err(|e| format!("Couldn't save {path}: {e}"))?;
    std::fs::rename(&tmp, &path).map_err(|e| format!("Couldn't save {path}: {e}"))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![evaluate, evaluate_sheet, plot_sheet, solve_figure, read_project, write_project])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
