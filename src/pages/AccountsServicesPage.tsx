import ConnectionsCenter from "../components/ConnectionsCenter";
import LincoPhoneAccess from "../components/LincoPhoneAccess";
import SectionTabs from "../components/SectionTabs";
import type { PageName } from "../types/index";

type AccountsServicesPageProps = {
  onPageChange: (page: PageName) => void;
};

export default function AccountsServicesPage({ onPageChange }: AccountsServicesPageProps) {
  return (
    <section className="my-ai-page">
      <header className="my-ai-header">
        <div>
          <p className="settings-kicker">My AI</p>
          <h1>Accounts &amp; Services</h1>
          <p>Manage shopping memberships, office accounts and other connected services.</p>
        </div>
      </header>
      <SectionTabs
        activePage="Accounts & Services"
        label="My AI sections"
        onPageChange={onPageChange}
        items={[
          { label: "AI Models", page: "My AI" },
          { label: "Agents", page: "Agents" },
          { label: "Accounts & Services", page: "Accounts & Services" },
        ]}
      />
      <ConnectionsCenter />
      <LincoPhoneAccess />
    </section>
  );
}
