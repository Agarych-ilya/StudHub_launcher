import {useEffect,useState} from 'react';
import {ChevronRight,Pause,Play} from 'lucide-react';

const worlds=[
  {src:'/world.png',alt:'Замок в горах Minecraft на закате'},
  {src:'/world-valley.png',alt:'Деревня и мельница в зелёной долине Minecraft'},
  {src:'/world-snow.png',alt:'Заснеженные горы и уютный дом в Minecraft'},
];
export function WorldCarousel(){
  const [slide,setSlide]=useState(0),[paused,setPaused]=useState(false);
  useEffect(()=>{
    if(paused)return;
    const timer=window.setInterval(()=>{if(!document.hidden)setSlide(i=>(i+1)%worlds.length)},5000);
    return()=>window.clearInterval(timer);
  },[paused]);
  const move=(n:number)=>setSlide(i=>(i+n+worlds.length)%worlds.length);
  return <section className="hero" data-slide={slide} role="region" aria-label="Миры Minecraft">
    <div className="hero-slides">{worlds.map((world,i)=><img key={world.src} src={world.src} alt={world.alt} className={'hero-slide '+(slide===i?'active':'')} aria-hidden={slide!==i}/>)}</div>
    <div className="hero-shade"/>
    <div className="hero-content"><div className="eyebrow"><span/>MINECRAFT, КОТОРЫЙ ОБЪЕДИНЯЕТ</div><h1>Твой мир.<br/><em>Твои правила.</em></h1><p>Исследуй. Строй. Найди свою команду.<br/>Твоя история в StudHub начинается здесь.</p><div className="hero-tags"><span>JAVA EDITION</span><span>1.21.1</span><span>NEOFORGE</span></div></div>
    <div className="carousel-controls"><button aria-label="Предыдущее изображение" onClick={()=>move(-1)}><ChevronRight className="previous" size={17}/></button><div className="carousel-dots">{worlds.map((_,i)=><button key={i} aria-label={'Показать изображение '+(i+1)} aria-pressed={slide===i} className={slide===i?'active':''} onClick={()=>setSlide(i)}/>)}</div><button aria-label="Следующее изображение" onClick={()=>move(1)}><ChevronRight size={17}/></button><button aria-label={paused?'Включить перелистывание':'Приостановить перелистывание'} aria-pressed={paused} onClick={()=>setPaused(v=>!v)}>{paused?<Play size={14}/>:<Pause size={14}/>}</button></div>
  </section>;
}
