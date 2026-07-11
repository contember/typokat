// A module export list must resolve module-local slots only: ambient Math is not
// a declaration this module may re-export. Cross-checked with tsc 6.0.3 --strict.

export { Math as M }; // error[TK2304]: Cannot find name 'Math'
