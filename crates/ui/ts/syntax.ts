import Parser from 'web-tree-sitter';

let parser: Parser | null = null;
const languages = new Map<string, Parser.Language>();

// Initialize Tree-sitter (must be called before use)
export async function initTreeSitter() {
    if (parser) return;
    
    await Parser.init({
        locateFile(scriptName: string, scriptDirectory: string) {
            return `/tree-sitter.wasm`; 
        },
    });
    
    parser = new Parser();
}

// Load a language grammar
export async function loadLanguage(langName: string, wasmPath: string) {
    if (languages.has(langName)) return;

    try {
        const lang = await Parser.Language.load(wasmPath);
        languages.set(langName, lang);
    } catch (e) {
        console.error(`[Syntax] Failed to load language ${langName}:`, e);
    }
}

// Highlight code (simplified - returns S-expression or token list)
// In a real impl, this would map nodes to colors.
// For now, we just verify we can parse.
export function parseCode(code: string, langName: string): string {
    if (!parser) return "Error: Parser not initialized";
    
    const lang = languages.get(langName);
    if (!lang) return "Error: Language not loaded";
    
    parser.setLanguage(lang);
    const tree = parser.parse(code);
    const root = tree.rootNode;
    
// Expose to window for Rust
(window as any).initTreeSitter = initTreeSitter;
(window as any).loadLanguage = loadLanguage;
(window as any).parseCode = parseCode;
