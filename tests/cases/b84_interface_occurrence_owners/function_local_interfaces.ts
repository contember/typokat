// WU3a acceptance: the manifest owns interface occurrence tickets.

function checkFunctionLocal(): void {
  interface UsedLocal {
    value: string;
  }
  interface UnusedLocal {
    unused: true;
  }
  const valid: UsedLocal = { value: "valid" };
  const invalid: UsedLocal = { value: 1 }; // error[TK2322]
}

const checkArrowLocal = (): void => {
  interface UsedLocal {
    value: string;
  }
  interface UnusedLocal {
    unused: true;
  }
  const valid: UsedLocal = { value: "valid" };
  const invalid: UsedLocal = { value: 1 }; // error[TK2322]
};

checkFunctionLocal();
checkArrowLocal();
