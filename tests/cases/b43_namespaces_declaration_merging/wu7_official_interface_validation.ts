// tsc 6.0.3 --strict --noEmit --pretty false --lib es5 --module commonjs:
// TS2411, TS2320, and TS2430 x2. Interface validation diagnostics are owned by
// the declaration that introduces the incompatibility, and a compatible own
// property reconciles otherwise-conflicting inherited properties.

interface Wu7MethodStringIndexIncompatible {
  [key: string]: number;
  incompatible(value: string): string; // error[TK2411]: Property 'incompatible' of type '(value: string) => string' is not assignable to 'string' index type 'number'
}

interface Wu7MethodStringIndexCompatible {
  [key: string]: (value: string) => string;
  compatible(value: string): string;
}

interface Wu7OverlayLeft {
  value: { left: number };
}
interface Wu7OverlayRight {
  value: { right: string };
}
interface Wu7CompatibleOwnOverlay extends Wu7OverlayLeft, Wu7OverlayRight {
  value: { left: number; right: string };
}
declare const wu7CompatibleOwnOverlay: Wu7CompatibleOwnOverlay;
const wu7CompatibleOwnOverlayLeft: number = wu7CompatibleOwnOverlay.value.left;
const wu7CompatibleOwnOverlayRight: string = wu7CompatibleOwnOverlay.value.right;

interface Wu7NoOverlayLeft {
  value: { left: number };
}
interface Wu7NoOverlayRight {
  value: { right: string };
}
interface Wu7NoOwnOverlay extends Wu7NoOverlayLeft, Wu7NoOverlayRight {} // error[TK2320]: cannot simultaneously extend types 'Wu7NoOverlayLeft' and 'Wu7NoOverlayRight'

interface Wu7IncompatibleOverlayLeft {
  value: { left: number };
}
interface Wu7IncompatibleOverlayRight {
  value: { right: string };
}
interface Wu7IncompatibleOwnOverlay extends Wu7IncompatibleOverlayLeft, Wu7IncompatibleOverlayRight { // error[TK2430]: Interface 'Wu7IncompatibleOwnOverlay' incorrectly extends interface 'Wu7IncompatibleOverlayLeft' | error[TK2430]: Interface 'Wu7IncompatibleOwnOverlay' incorrectly extends interface 'Wu7IncompatibleOverlayRight'
  value: boolean;
}
