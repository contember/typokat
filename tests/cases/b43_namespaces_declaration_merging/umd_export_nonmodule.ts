// tsc 6.0.3 --strict --module commonjs: TS1314 without an external-module marker.
export as namespace InvalidScriptUmd; // error[TK1314]: Global module exports may only appear in module files
