// A local type slot may be exported, but must not acquire ambient Math's value
// slot. Cross-checked with tsc 6.0.3 --strict.

type Math = { value: number };
export { Math as M };
