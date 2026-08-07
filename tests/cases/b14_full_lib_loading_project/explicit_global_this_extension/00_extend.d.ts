// tsc 6.0.3 reports TS2397. This shipped marker pins the matching TK code;
// direct route-parity tests pin synthetic-root routing.
declare namespace globalThis { // error[TK2397]
  let B14ExplicitGlobalThis: {
    enabled: boolean;
  };
}
