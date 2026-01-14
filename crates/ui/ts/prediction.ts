/**
 * Client-Side Prediction Engine
 * 
 * Uses a lightweight Lua VM (wasmoon) to predict cursor movement
 * for zero-latency h/j/k/l navigation.
 */

// Note: This module requires wasmoon to be loaded via script tag or bundler
// <script src="https://unpkg.com/wasmoon/dist/glue.js"></script>

interface CursorState {
  row: number;
  col: number;
  mode: 'normal' | 'insert' | 'visual' | 'command';
}

interface PredictionResult {
  newCursor: CursorState;
  predicted: boolean;
}

// Lua script for cursor prediction
const PREDICTION_LUA = `
-- Cursor state
cursor = { row = 0, col = 0 }
mode = "normal"
buffer_lines = 0
line_length = 80

function set_state(row, col, m, lines, linelen)
  cursor.row = row
  cursor.col = col
  mode = m
  buffer_lines = lines
  line_length = linelen
end

function predict_key(key)
  if mode ~= "normal" then
    return nil -- Only predict in normal mode
  end
  
  local predicted = false
  
  if key == "h" then
    cursor.col = math.max(0, cursor.col - 1)
    predicted = true
  elseif key == "l" then
    cursor.col = math.min(line_length - 1, cursor.col + 1)
    predicted = true
  elseif key == "j" then
    cursor.row = math.min(buffer_lines - 1, cursor.row + 1)
    predicted = true
  elseif key == "k" then
    cursor.row = math.max(0, cursor.row - 1)
    predicted = true
  elseif key == "0" then
    cursor.col = 0
    predicted = true
  elseif key == "$" then
    cursor.col = line_length - 1
    predicted = true
  elseif key == "gg" then
    cursor.row = 0
    predicted = true
  elseif key == "G" then
    cursor.row = buffer_lines - 1
    predicted = true
  elseif key == "w" then
    -- Approximate word jump
    cursor.col = math.min(line_length - 1, cursor.col + 5)
    predicted = true
  elseif key == "b" then
    cursor.col = math.max(0, cursor.col - 5)
    predicted = true
  end
  
  if predicted then
    return { row = cursor.row, col = cursor.col, predicted = true }
  end
  return nil
end
`;

type LuaFactory = () => Promise<LuaEngine>;
interface LuaEngine {
  doString(code: string): Promise<any>;
  global: {
    get(name: string): any;
    set(name: string, value: any): void;
  };
}

declare global {
  interface Window {
    LuaFactory?: LuaFactory;
  }
}

export class PredictionEngine {
  private lua: LuaEngine | null = null;
  private ready: boolean = false;
  private cursorState: CursorState = { row: 0, col: 0, mode: 'normal' };
  private bufferLines: number = 100;
  private lineLength: number = 80;

  /**
   * Initialize the Lua VM
   */
  async init(): Promise<boolean> {
    try {
      if (!window.LuaFactory) {
        console.warn('[Prediction] Wasmoon not loaded, prediction disabled');
        return false;
      }

      const factory = window.LuaFactory;
      this.lua = await factory();
      await this.lua.doString(PREDICTION_LUA);
      this.ready = true;
      console.log('[Prediction] Lua VM initialized');
      return true;
    } catch (e) {
      console.error('[Prediction] Failed to initialize:', e);
      return false;
    }
  }

  /**
   * Update cursor state from server
   */
  updateState(row: number, col: number, mode: string, bufferLines?: number, lineLength?: number): void {
    this.cursorState = { row, col, mode: mode as CursorState['mode'] };
    if (bufferLines !== undefined) this.bufferLines = bufferLines;
    if (lineLength !== undefined) this.lineLength = lineLength;

    if (this.lua && this.ready) {
      try {
        const setState = this.lua.global.get('set_state');
        if (setState) {
          setState(row, col, mode, this.bufferLines, this.lineLength);
        }
      } catch (e) {
        // Ignore errors
      }
    }
  }

  /**
   * Predict cursor position for a key press
   */
  predict(key: string): PredictionResult | null {
    if (!this.ready || !this.lua) {
      return null;
    }

    // Only predict in normal mode for h/j/k/l
    if (this.cursorState.mode !== 'normal') {
      return null;
    }

    // Simple keys we can predict
    const predictableKeys = ['h', 'j', 'k', 'l', '0', '$', 'w', 'b', 'G'];
    if (!predictableKeys.includes(key)) {
      return null;
    }

    try {
      const predictKey = this.lua.global.get('predict_key');
      if (!predictKey) return null;

      const result = predictKey(key);
      if (result && result.predicted) {
        return {
          newCursor: {
            row: result.row,
            col: result.col,
            mode: this.cursorState.mode,
          },
          predicted: true,
        };
      }
    } catch (e) {
      console.error('[Prediction] Error:', e);
    }

    return null;
  }

  /**
   * Reconcile with server state
   */
  reconcile(serverRow: number, serverCol: number): void {
    // Server is source of truth - update local state
    this.cursorState.row = serverRow;
    this.cursorState.col = serverCol;
    this.updateState(serverRow, serverCol, this.cursorState.mode);
  }

  isReady(): boolean {
    return this.ready;
  }
}

// Singleton instance
let predictionEngine: PredictionEngine | null = null;

export function getPredictionEngine(): PredictionEngine {
  if (!predictionEngine) {
    predictionEngine = new PredictionEngine();
  }
  return predictionEngine;
}
