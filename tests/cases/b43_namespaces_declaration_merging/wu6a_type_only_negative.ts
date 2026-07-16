// WU6A type-only oracle: tsc 6.0.3 --strict --noEmit --pretty false --lib es5 --module commonjs.
// Every value demand reports TS2708; qualified type use remains clean.

namespace Wu6aTypeOnly {
  export interface Shape {
    value: number;
  }
  export type Alias = Shape;
}

const wu6aTypeOnlyQualified: Wu6aTypeOnly.Shape = { value: 1 };
const wu6aTypeOnlyAlias = Wu6aTypeOnly; // error[TK2708]: Cannot use namespace 'Wu6aTypeOnly' as a value
Wu6aTypeOnly; // error[TK2708]: Cannot use namespace 'Wu6aTypeOnly' as a value
Wu6aTypeOnly.Shape; // error[TK2708]: Cannot use namespace 'Wu6aTypeOnly' as a value
Wu6aTypeOnly(); // error[TK2708]: Cannot use namespace 'Wu6aTypeOnly' as a value
new Wu6aTypeOnly(); // error[TK2708]: Cannot use namespace 'Wu6aTypeOnly' as a value

declare namespace Wu6aAmbientTypeOnly {
  interface Shape {
    value: number;
  }
  type Alias = Shape;
}

const wu6aAmbientTypeOnlyQualified: Wu6aAmbientTypeOnly.Shape = { value: 1 };
const wu6aAmbientTypeOnlyAlias = Wu6aAmbientTypeOnly; // error[TK2708]: Cannot use namespace 'Wu6aAmbientTypeOnly' as a value
Wu6aAmbientTypeOnly; // error[TK2708]: Cannot use namespace 'Wu6aAmbientTypeOnly' as a value
Wu6aAmbientTypeOnly.Shape; // error[TK2708]: Cannot use namespace 'Wu6aAmbientTypeOnly' as a value
Wu6aAmbientTypeOnly(); // error[TK2708]: Cannot use namespace 'Wu6aAmbientTypeOnly' as a value
new Wu6aAmbientTypeOnly(); // error[TK2708]: Cannot use namespace 'Wu6aAmbientTypeOnly' as a value
