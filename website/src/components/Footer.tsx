export function Footer() {
  return (
    <footer className="site-footer">
      <div className="site-container site-footer__inner text-caption">
        <span>© {new Date().getFullYear()} SanctuaryMix</span>
        <span>Allen &amp; Heath and dLive are trademarks of their owners. Dante is a trademark of Audinate.</span>
      </div>
    </footer>
  );
}
