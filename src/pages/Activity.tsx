import { AlertTriangle, CheckCircle2, Info, ListChecks, XCircle } from "lucide-react";
import { dateTime } from "../format";
import { useStore } from "../store";
import { Empty } from "../ui";

const ICONS = {
  success: <CheckCircle2 size={15} />,
  warning: <AlertTriangle size={15} />,
  error: <XCircle size={15} />,
  info: <Info size={15} />,
};

export default function Activity() {
  const { activity } = useStore();
  return (
    <div className="page">
      <div className="panel">
        {activity.length === 0 ? (
          <Empty icon={<ListChecks size={20} />} title="No activity yet">Sharing, pairing and transfer events are recorded here.</Empty>
        ) : (
          activity.map((a) => (
            <div className="activity-item" key={a.id}>
              <span className={`lvl ${a.level}`} aria-label={a.level}>{ICONS[a.level] ?? ICONS.info}</span>
              <span className="grow selectable">{a.message}</span>
              <span className="faint small num" style={{ whiteSpace: "nowrap" }}>{dateTime(a.ts)}</span>
            </div>
          ))
        )}
      </div>
      <span className="hint">The log never includes file contents or invitation secrets.</span>
    </div>
  );
}
