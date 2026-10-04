import { ArrowRight, Check, Gift, Info } from "lucide-react";
import { useState } from "react";
import {
  FOUNDING_OFFER,
  PLANS,
  PRICES_ARE_PLACEHOLDERS,
  PRICING_FOOTNOTE,
  TRIAL_DAYS,
  formatPrice,
  maxYearlySavingsPercent,
  monthsFreeYearly,
  planHref,
  pricePerMonth,
  yearlySavingsPercent,
  type Billing,
} from "../pricing";

interface Props {
  /** Relative path back to the site root from the current page. */
  root: string;
}

export function Pricing({ root }: Props) {
  const [billing, setBilling] = useState<Billing>("yearly");
  const bestSaving = maxYearlySavingsPercent();
  const monthsFree = monthsFreeYearly();

  return (
    <section id="pricing" className="section section--surface" aria-labelledby="pricing-heading">
      <div className="site-container">
        <div className="pricing__head">
          <div>
            <h2 id="pricing-heading" className="section__title">Pricing</h2>
            <p className="section__lede">
              One price per church, not per seat. Every plan starts with a {TRIAL_DAYS}-day free trial, no card
              needed.
            </p>
          </div>
          <div className="pricing__toggle">
            <div className="sm-seg" role="group" aria-label="Billing period">
              <button type="button" aria-pressed={billing === "monthly"} onClick={() => setBilling("monthly")}>
                Monthly
              </button>
              <button type="button" aria-pressed={billing === "yearly"} onClick={() => setBilling("yearly")}>
                Yearly
              </button>
            </div>
            {monthsFree > 0 ? (
              <span className="pricing__save text-caption">Yearly: {monthsFree} months free</span>
            ) : bestSaving > 0 ? (
              <span className="pricing__save text-caption">Save up to {bestSaving}% yearly</span>
            ) : null}
          </div>
        </div>

        {PRICES_ARE_PLACEHOLDERS ? (
          <p className="preview-note text-caption pricing__note" role="note">
            <Info aria-hidden="true" />
            Sample prices. Final pricing will be posted before launch.
          </p>
        ) : null}

        {FOUNDING_OFFER ? (
          <p className="founding text-caption">
            <Gift aria-hidden="true" />
            <span>
              <strong>Founding church offer.</strong> The first {FOUNDING_OFFER.spots} churches to subscribe get{" "}
              {FOUNDING_OFFER.percentOff}% off any plan for as long as they stay subscribed.
            </span>
          </p>
        ) : null}

        <ul className="plans">
          {PLANS.map((plan) => {
            const saving = yearlySavingsPercent(plan);
            return (
              <li key={plan.id} className={`plan${plan.featured ? " plan--featured" : ""}`}>
                <div className="plan__top">
                  <h3 className="text-heading">{plan.name}</h3>
                  {plan.featured ? <span className="plan__tag text-label">Most popular</span> : null}
                </div>
                <p className="feature__body">{plan.audience}</p>
                <p className="plan__price">
                  <span className="plan__amount">{formatPrice(pricePerMonth(plan, billing))}</span>
                  <span className="plan__per text-caption">per month</span>
                </p>
                <p className="plan__billed text-caption">
                  {billing === "yearly"
                    ? `${formatPrice(plan.yearly)} billed yearly${saving > 0 ? `, save ${saving}%` : ""}`
                    : `Billed monthly, or ${formatPrice(plan.yearly)} a year`}
                </p>
                {plan.note ? <p className="plan__billed text-caption">{plan.note}</p> : null}
                <a
                  className={`sm-btn sm-btn--lg plan__cta${plan.featured ? " sm-btn--primary" : ""}`}
                  href={planHref(root, plan, billing)}
                >
                  Start free trial
                  <ArrowRight aria-hidden="true" />
                </a>
                <ul className="plan__features">
                  {plan.features.map((f) => (
                    <li key={f}>
                      <Check aria-hidden="true" />
                      {f}
                    </li>
                  ))}
                </ul>
              </li>
            );
          })}
        </ul>
        <p className="pricing__foot text-caption">{PRICING_FOOTNOTE}</p>
      </div>
    </section>
  );
}
