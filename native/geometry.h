#pragma once
#include <cmath>
#include <algorithm>
namespace cast {
struct Rect { int x, y, width, height; };
inline Rect fit(int available_width, int available_height, int source_width, int source_height) {
    if (available_width <= 0 || available_height <= 0 || source_width <= 0 || source_height <= 0) return {0,0,0,0};
    const double scale=std::min(double(available_width)/source_width,double(available_height)/source_height);
    const int width=std::max(1,int(std::round(source_width*scale)));
    const int height=std::max(1,int(std::round(source_height*scale)));
    return {(available_width-width)/2,(available_height-height)/2,width,height};
}
inline bool normalize(int x,int y,int width,int height,double& nx,double& ny) {
    if(width<=0||height<=0||x<0||y<0||x>=width||y>=height) return false;
    nx=width==1?0.0:double(x)/(width-1);ny=height==1?0.0:double(y)/(height-1);return true;
}
}
