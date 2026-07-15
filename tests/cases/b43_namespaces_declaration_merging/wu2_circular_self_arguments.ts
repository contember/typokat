// tsc 6.0.3 --strict --noEmit additionally reports TS2315 for both self applications
// and TS2313 for the mapped key; typokat deliberately omits those existing divergences.
type ConditionalSelf<Argument> = ConditionalSelf<MissingConditional.Argument> extends string // error[TK2456]: Type alias 'ConditionalSelf' circularly references itself | error[TK2503]: Cannot find namespace 'MissingConditional'
  ? MissingConditional.TrueBranch // error[TK2503]: Cannot find namespace 'MissingConditional'
  : MissingConditional.FalseBranch; // error[TK2503]: Cannot find namespace 'MissingConditional'

type MappedSelf<Argument> = { // error[TK2456]: Type alias 'MappedSelf' circularly references itself
  [Key in keyof MappedSelf<MissingMapped.Argument>]: MissingMapped.Value; // error[TK2503]: Cannot find namespace 'MissingMapped' | error[TK2503]: Cannot find namespace 'MissingMapped'
};
