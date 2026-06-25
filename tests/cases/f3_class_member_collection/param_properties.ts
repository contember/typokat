// F3 / backlog 01 — constructor PARAMETER PROPERTIES become real members.
// A constructor parameter carrying an accessibility / `readonly` modifier
// (`public`/`private`/`protected`/`readonly`) declares an instance member with
// that modifier and the parameter's annotated type — in addition to being a
// constructor parameter. Before this fix the member was dropped, so every access
// over-reported TK2339. Cross-checked against tsc 6.0.3 (TS2322/2341/2445/2540).

class C {
  constructor(
    public x: number,
    private y: string,
    protected z: boolean,
    readonly w: number,
  ) {}
  self(): void {
    const a: string = this.y;  // ok — private readable inside the declaring class
    const b: boolean = this.z; // ok — protected readable inside the declaring class
  }
}

const c = new C(1, "a", true, 2);
const n: number = c.x;   // ok — public member, type number
const bad: string = c.x; // error[TK2322]: not assignable to type 'string'
c.y;                     // error[TK2341]: Property 'y' is private
c.z;                     // error[TK2445]: Property 'z' is protected
c.w = 5;                 // error[TK2540]: Cannot assign to 'w' because it is a read-only property
