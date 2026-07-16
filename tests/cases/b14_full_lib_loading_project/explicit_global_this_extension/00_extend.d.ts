// tsc 6.0.3 reports TS2397. This disabled acceptance marker plans the matching TK code;
// WU3 owns its production diagnostic and synthetic-root routing.
declare namespace globalThis { // error[TK2397]
  let B14ExplicitGlobalThis: {
    enabled: boolean;
  };
}
