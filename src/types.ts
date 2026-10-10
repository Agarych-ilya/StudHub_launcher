export type Deployment = {api_url:string;manifest_url:string;trusted_origins:string[];manifest_public_key:string;server_address:string;launcher_update_url:string;launcher_public_key:string;spawn_status_url:string};
export type Settings = {game_dir:string;java_path:string;ram_mb:number;nickname:string;auto_connect:boolean;auto_update:boolean;deployment:Deployment};
export type Mod = {file:string;ids:string[];managed:boolean;issue:string|null};
export type DriveStatus = {status:string;checked_at?:number;error?:string;changes:string[];files:{id:string;name:string;size:number;modified:number}[]};
export type GitHubUpdate = {status:"checking"|"no_releases"|"pending_asset"|"available"|"current"|"error";version?:string;release_url?:string;notes?:string;published_at?:string;error?:string};
export type Snapshot = {drive:DriveStatus;settings:Settings;effective_ram_mb:number;pid:number;busy:boolean;minecraft:{name:string;id:string;needs_refresh?:boolean}|null;server:{online:boolean;players?:number;max_players?:number;version?:string;error?:string;spawn_players?:number;spawn_error?:string;checked_at?:number};news:{items:{title:string;body?:string;date?:string}[];error?:string};pack_version:string|null;neoforge:string|null;mods:Mod[];logs:string[];app_version:string;github_update:GitHubUpdate;update:{version:string;artifact?:unknown}|null};
export type Progress = {stage:string;done:number;total:number;speed:number;eta:number};
export function percent(p:Progress|null) {return p&&p.total>0?Math.max(0,Math.min(100,Math.round(p.done/p.total*100))):0}
export function bytes(n:number) {return n>=1048576?(n/1048576).toFixed(1)+' МБ':(n/1024).toFixed(0)+' КБ'}
