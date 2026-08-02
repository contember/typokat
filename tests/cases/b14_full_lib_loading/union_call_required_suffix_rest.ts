// tsc 6.0.3 --strict --target es2025: every call is accepted.

type HeadA = { readonly headA: true };
type MiddleB = { readonly middleB: true };
type TailC = { readonly tailC: true };

type HeadD = { readonly headD: true };
type MiddleE = { readonly middleE: true };
type TailF = { readonly tailF: true };

declare const requiredSuffixRest:
  | ((...args: [head: HeadA, ...middle: MiddleB[], tail: TailC]) => "left")
  | ((...args: [head: HeadD, ...middle: MiddleE[], tail: TailF]) => "right");

declare const h: HeadA & HeadD;
declare const m: MiddleB & MiddleE;
declare const t: TailC & TailF;

const zeroMiddle: "left" | "right" = requiredSuffixRest(h, t);
const oneMiddle: "left" | "right" = requiredSuffixRest(h, m, t);
const twoMiddle: "left" | "right" = requiredSuffixRest(h, m, m, t);
