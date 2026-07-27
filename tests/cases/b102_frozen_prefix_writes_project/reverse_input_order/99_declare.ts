// The declaring file sorts (and is fed to the driver) after its consumer on purpose.
declare var b102LateGlobal: number;

interface B102LateShape {
  name: string;
}

declare function b102LateFn(input: number): string;

class B102LateClass {
  value: number = 1;
}

declare namespace B102LateNamespace {
  const member: number;
}

type B102LateAlias = string;
