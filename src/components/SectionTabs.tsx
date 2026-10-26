import type { PageName } from "../types/index";

type SectionTabsProps = {
  activePage: PageName;
  items: Array<{ label: string; page: PageName }>;
  onPageChange: (page: PageName) => void;
  label: string;
};

export default function SectionTabs({ activePage, items, onPageChange, label }: SectionTabsProps) {
  return (
    <nav className="section-tabs" aria-label={label}>
      {items.map((item) => (
        <button
          key={item.page}
          type="button"
          className={activePage === item.page ? "section-tab section-tab-active" : "section-tab"}
          aria-current={activePage === item.page ? "page" : undefined}
          onClick={() => onPageChange(item.page)}
        >
          {item.label}
        </button>
      ))}
    </nav>
  );
}
