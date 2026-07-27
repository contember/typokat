// From backlog 95 ("Three lines, no generics" — the generic arrow is what makes the
// argument supersedable). Hand-written anchor for the shape the generator aims at:
// an arrow argument whose returned value is the enclosing callback's contextually
// typed parameter. 412f321 reported nothing here; 412f321~1 and tsc both reject.
declare function plain(step: (value: number) => void): void;
declare function wantsStrFn(f: () => string): void;
plain(p0 => { wantsStrFn(<U,>() => p0); });
