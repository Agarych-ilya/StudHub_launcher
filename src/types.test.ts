import {describe,it,expect} from 'vitest';
import {percent,bytes} from './types';
describe('Параметры интерфейса',()=>{
  it('не отображает некорректный прогресс',()=>{expect(percent(null)).toBe(0);expect(percent({stage:'x',done:120,total:100,speed:0,eta:0})).toBe(100)});
  it('показывает скорость в понятных единицах',()=>expect(bytes(1048576)).toBe('1.0 МБ'));
});
