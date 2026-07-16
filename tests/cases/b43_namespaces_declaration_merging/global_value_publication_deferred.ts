// tsc 6.0.3 --strict: clean; global value publication has a dedicated future owner.
export {};

declare global { // incomplete[decl/global-declaration/self]: global augmentation value publication not modeled
  const WU5GlobalConst: { value: number };
  function wu5GlobalFunction(value: number): string;
  class WU5GlobalClass { value: number }
}
