// The supplied logo files, swapped by theme. Never rebuilt in live type.
export function Wordmark({ root }: { root: string }) {
  return (
    <>
      <img className="wordmark wordmark--on-dark" src={`${root}brand/sanctuarymix-wordmark-dark.svg`} alt="SanctuaryMix" />
      <img className="wordmark wordmark--on-light" src={`${root}brand/sanctuarymix-wordmark-light.svg`} alt="SanctuaryMix" />
    </>
  );
}
