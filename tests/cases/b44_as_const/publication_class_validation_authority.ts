interface AuthorityKeys { alpha: number; beta: string }
interface AuthorityHolder { key: keyof AuthorityKeys }
class AnnotationAuthority {
  declared: AuthorityHolder = { key: "alpha" } satisfies AuthorityHolder;
  invalid: number = "bad" satisfies string; // error[TK2322]
  returnValue = ((): number => "bad") satisfies () => number; // error[TK2322]
  validReturn = ((): AuthorityHolder => ({ key: "alpha" })) satisfies () => AuthorityHolder;
  exact = { key: "alpha" } satisfies { key: "alpha" | "beta" };
}
const annotationAuthority = new AnnotationAuthority();
let authorityExactCopy = annotationAuthority.exact.key;
authorityExactCopy = "beta"; // error[TK2322]

class TemplateAuthority<T> {
  values: T[];
  count = (() => 1) satisfies () => number;
  echo = ((value: T) => value) satisfies (value: T) => T;
  constructor(values: T[]) { this.values = values; }
}
const templateAuthority = new TemplateAuthority<string>(["a"]);
const templateValue: string = templateAuthority.values[0];
const invalidTemplateValue: number = templateAuthority.values[0]; // error[TK2322]
const echoedTemplateValue: string = templateAuthority.echo("a");
const invalidEchoedValue: number = templateAuthority.echo("a"); // error[TK2322]
