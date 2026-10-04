// Plans and prices shown on the site. This is the only place prices live:
// change a number here and the pricing section, the yearly discount and the
// account page all follow.
//
// There's no checkout yet. Choosing a plan opens the sample account page with
// ?plan=<id>&billing=<monthly|yearly>, which a real checkout will replace.

export type Billing = "monthly" | "yearly";

export interface Plan {
  id: string;
  name: string;
  /** One line on who the plan is for. */
  audience: string;
  /** Price per month when billed monthly, in whole US dollars. */
  monthly: number;
  /** Price for a full year when billed yearly, in whole US dollars. */
  yearly: number;
  features: string[];
  /** The plan most churches should pick; drawn with the primary button. */
  featured?: boolean;
}

/**
 * While true, the section says the prices are samples. Set to false once
 * Zach approves the final numbers.
 */
export const PRICES_ARE_PLACEHOLDERS = true;

export const CURRENCY = "USD";

export const TRIAL_DAYS = 14;

export const PLANS: Plan[] = [
  {
    id: "starter",
    name: "Starter",
    audience: "One booth with a volunteer team.",
    monthly: 29,
    yearly: 290,
    features: ["1 booth computer", "Assist level suggestions", "Scene and channel profiles", "Email support"],
  },
  {
    id: "church",
    name: "Church",
    audience: "A main room with services every week.",
    monthly: 59,
    yearly: 590,
    featured: true,
    features: [
      "2 booth computers",
      "Everything in Starter",
      "Auto mode per channel",
      "Service logs and mix history",
      "Priority support",
    ],
  },
  {
    id: "multisite",
    name: "Multi-site",
    audience: "Several rooms or campuses.",
    monthly: 129,
    yearly: 1290,
    features: ["Up to 6 booth computers", "Everything in Church", "Shared profiles across rooms", "Onboarding call"],
  },
];

export const findPlan = (id: string | null) => PLANS.find((p) => p.id === id);

/** Whole-number percent saved by paying yearly instead of twelve months. */
export const yearlySavingsPercent = (plan: Plan) => Math.round((1 - plan.yearly / (plan.monthly * 12)) * 100);

/** The best yearly saving across plans, for the toggle label. */
export const maxYearlySavingsPercent = () => Math.max(...PLANS.map(yearlySavingsPercent));

const money = new Intl.NumberFormat("en-US", { style: "currency", currency: CURRENCY, maximumFractionDigits: 0 });
const moneyCents = new Intl.NumberFormat("en-US", { style: "currency", currency: CURRENCY, minimumFractionDigits: 2 });

/** "$29", or "$24.17" when a yearly price doesn't divide evenly by month. */
export function formatPrice(amount: number) {
  return Number.isInteger(amount) ? money.format(amount) : moneyCents.format(amount);
}

/** The per-month figure shown large on a plan card. */
export const pricePerMonth = (plan: Plan, billing: Billing) =>
  billing === "monthly" ? plan.monthly : Math.round((plan.yearly / 12) * 100) / 100;

/** Link from a plan card into the sample account flow. */
export const planHref = (root: string, plan: Plan, billing: Billing) =>
  `${root}account/?plan=${encodeURIComponent(plan.id)}&billing=${billing}`;
