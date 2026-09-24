//! Security Case automatizado (US02 Task 2.4): NENHUM arquivo do frontend
//! (`desktop-client/src`) pode referenciar armazenamento web do navegador —
//! o JWT vive só na memória do React + cofre do SO via IPC. Falha no CI se
//! alguém introduzir o padrão (equivale a inspecionar o Application tab e
//! achar o token). Convenção: nem comentários usam os literais proibidos.

use std::path::{Path, PathBuf};

const FORBIDDEN: &[&str] = &["localStorage", "sessionStorage"];

fn ts_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let entries = std::fs::read_dir(dir).unwrap();
    for entry in entries {
        let path = entry.unwrap().path();
        if path.is_dir() {
            // Dependências e build nunca são auditados aqui.
            if matches!(
                path.file_name().and_then(|n| n.to_str()),
                Some("node_modules" | "dist" | "target")
            ) {
                continue;
            }
            ts_files(&path, out);
        } else if matches!(path.extension().and_then(|e| e.to_str()), Some("ts" | "tsx" | "js" | "jsx")) {
            out.push(path);
        }
    }
}

#[test]
fn frontend_sem_web_storage_em_texto_plano() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("src");
    let mut files = Vec::new();
    ts_files(&root, &mut files);
    assert!(!files.is_empty(), "frontend `src/` não encontrado para auditoria");
    for path in &files {
        let source = std::fs::read_to_string(path).unwrap();
        for token in FORBIDDEN {
            assert!(
                !source.contains(token),
                "{} usa `{}` — JWT só em memória + cofre via IPC",
                path.display(),
                token
            );
        }
    }
}
