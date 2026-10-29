import type {ReactNode} from 'react';
type Props={workspace:ReactNode};
export function ProductFrame(props:Props){return <main id="frame"><header>fixture</header>{props.workspace}<footer>tail</footer></main>}
