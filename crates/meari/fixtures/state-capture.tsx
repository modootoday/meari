import { useState } from "react";
type Props = {};
export function CaptureDemo(props: Props) {
  const [count, setCount] = useState(0);
  const [step, setStep] = useState(2);
  const [label, setLabel] = useState("A");
  return <div>
    <p id="count">{count}</p>
    <p id="label">{label}</p>
    <button id="capture" onClick={() => setCount(count + 1)}>Capture</button>
    <button id="functional" onClick={() => setCount(previous => previous * 2 + step)}>Functional</button>
    <button id="step" onClick={() => setStep(10)}>Step</button>
    <button id="append" onClick={() => setLabel(previous => previous + "!")}>Append</button>
  </div>;
}
