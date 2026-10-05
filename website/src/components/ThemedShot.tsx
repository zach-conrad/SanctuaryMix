interface Props {
  /** Relative path back to the site root from the current page. */
  root: string;
  /** Base name in public/screenshots; `-dark.png` and `-light.png` are added. */
  name: string;
  alt: string;
  lazy?: boolean;
  className?: string;
}

// An app screenshot in the page's theme: the dark capture by default, the light one in the light theme.
export function ThemedShot({ root, name, alt, lazy, className }: Props) {
  const src = (theme: string) => `${root}screenshots/${name}-${theme}.png`;
  const loading = lazy ? "lazy" : undefined;
  return (
    <figure className={`shot${className ? ` ${className}` : ""}`}>
      <img className="on-dark" src={src("dark")} alt={alt} loading={loading} width={1440} height={900} />
      <img className="on-light" src={src("light")} alt={alt} loading="lazy" width={1440} height={900} />
    </figure>
  );
}
