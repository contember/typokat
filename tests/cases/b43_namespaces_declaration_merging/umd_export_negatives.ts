// tsc 6.0.3 --strict --module commonjs: TS1315 for a module-form .ts source.
export as namespace InvalidSourceUmd; // error[TK1315]: Global module exports may only appear in declaration files
export {};
